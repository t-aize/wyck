import { useState } from "react";
import { COMMANDS, type CommandContext, findCommand } from "../../commands/index.ts";
import type { GetPositionsResult } from "../../ctrader/schemas.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { useFeedback } from "../context/FeedbackContext.tsx";
import type { CancelConfirm } from "./useCancelConfirm.ts";
import type { ModifyConfirm } from "./useModifyConfirm.ts";
import type { TradeConfirm } from "./useTradeConfirm.ts";

export interface CommandRouter {
  runCommand: (raw: string) => void;
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
}): CommandRouter {
  const {
    positions,
    refreshMarket,
    refreshNews,
    onReconfigure,
    tradeConfirm,
    modifyConfirm,
    cancelConfirm,
  } = opts;
  const { client, symbolId } = useCtrader();
  const { setFeedback } = useFeedback();

  // Réglé via la commande `risk`, jamais persisté : repart à zéro à chaque lancement plutôt que de
  // continuer à trader silencieusement sur un risque défini une session précédente et oublié.
  const [defaultRiskPercent, setDefaultRiskPercent] = useState<number>();

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
      setFeedback,
      refreshMarket,
      refreshNews,
      onReconfigure,
      proposeTrade: tradeConfirm.proposeTrade,
      proposeModify: modifyConfirm.proposeModify,
      proposeCancel: cancelConfirm.proposeCancel,
      commands: COMMANDS,
    };
    command.run(args, ctx);
  }

  return { runCommand };
}
