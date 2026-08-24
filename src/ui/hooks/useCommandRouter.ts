import { useState } from "react";
import { COMMANDS, findCommand } from "../../commands/registry.ts";
import type { CommandContext } from "../../commands/types.ts";
import type { GetPositionsResult } from "../../ctrader/schemas.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { useFeedback } from "../context/FeedbackContext.tsx";
import type { CancelConfirm } from "./useCancelConfirm.ts";
import type { CloseConfirm } from "./useCloseConfirm.ts";
import type { ModifyConfirm } from "./useModifyConfirm.ts";
import type { PositionAmendConfirm } from "./usePositionAmendConfirm.ts";
import type { TradeConfirm } from "./useTradeConfirm.ts";

interface CommandRouter {
  runCommand: (raw: string) => void;
  atrMode: boolean;
  toggleAtrMode: () => void;
}

/**
 * Découpe la ligne tapée, résout la commande dans le registre (`commands/registry.ts`) et lui
 * délègue tout le reste (parsing + exécution) — un des 4 hooks issus de l'éclatement de
 * useOrderActions.ts. Ne possède plus lui-même qu'un seul bout d'état propre au routeur :
 * `defaultRiskPercent`, car il est partagé par plusieurs commandes (`risk` le règle, `trade` le lit).
 */
export function useCommandRouter(opts: {
  positions: GetPositionsResult | undefined;
  refreshMarket: () => Promise<void>;
  refreshNews: (options?: { force?: boolean }) => Promise<void>;
  onReconfigure: () => void;
  tradeConfirm: Pick<TradeConfirm, "proposeTrade">;
  modifyConfirm: Pick<ModifyConfirm, "proposeModify">;
  cancelConfirm: Pick<CancelConfirm, "proposeCancel">;
  positionAmendConfirm: Pick<PositionAmendConfirm, "proposePositionAmend">;
  closeConfirm: Pick<CloseConfirm, "proposeClose">;
}): CommandRouter {
  const {
    positions,
    refreshMarket,
    refreshNews,
    onReconfigure,
    tradeConfirm,
    modifyConfirm,
    cancelConfirm,
    positionAmendConfirm,
    closeConfirm,
  } = opts;
  const { client, symbolId } = useCtrader();
  const { setFeedback } = useFeedback();

  // Réglé via la commande `risk`, jamais persisté : repart à zéro à chaque lancement plutôt que de
  // continuer à trader silencieusement sur un risque défini une session précédente et oublié.
  const [defaultRiskPercent, setDefaultRiskPercent] = useState<number>();

  // Basculé par Shift+Tab (cf. useTerminalShortcuts.ts), lu par `trade` via ctx.atrMode.
  const [atrMode, setAtrMode] = useState(false);
  function toggleAtrMode() {
    setAtrMode((v) => !v);
  }

  function runCommand(raw: string) {
    const trimmed = raw.trim();
    if (!trimmed) return;
    const [commandRaw, ...args] = trimmed.split(/\s+/);
    const name = commandRaw?.toLowerCase() ?? "";

    const command = findCommand(name);
    if (!command) {
      setFeedback({ kind: "error", message: `commande inconnue : ${name} — help pour la liste` });
      return;
    }

    const ctx: CommandContext = {
      client,
      symbolId,
      positions,
      defaultRiskPercent,
      setDefaultRiskPercent,
      atrMode,
      setFeedback,
      refreshMarket,
      refreshNews,
      onReconfigure,
      proposeTrade: tradeConfirm.proposeTrade,
      proposeModify: modifyConfirm.proposeModify,
      proposeCancel: cancelConfirm.proposeCancel,
      proposePositionAmend: positionAmendConfirm.proposePositionAmend,
      proposeClose: closeConfirm.proposeClose,
      commands: COMMANDS,
    };
    command.run(args, ctx);
  }

  return { runCommand, atrMode, toggleAtrMode };
}
