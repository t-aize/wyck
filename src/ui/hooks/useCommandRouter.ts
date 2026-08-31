import { useState } from "react";
import { COMMANDS, findCommand } from "../../commands/registry.ts";
import type { CommandContext } from "../../commands/types.ts";
import type { TrendbarPeriod } from "../../constants.ts";
import type { GetPositionsResult } from "../../ctrader/schemas.ts";
import { writeConfig } from "../../settings.ts";
import { fsRuntime } from "../../utils/effectRuntime.ts";
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
  atrRefreshEnabled: boolean;
  setAtrRefreshEnabled: (enabled: boolean) => void;
  atrPeriod: number;
  atrTimeframe: TrendbarPeriod;
  setAtrPeriod: (period: number) => void;
  setAtrTimeframe: (timeframe: TrendbarPeriod) => void;
}

/**
 * Découpe la ligne tapée, résout la commande dans le registre (`commands/registry.ts`) et lui
 * délègue tout le reste (parsing + exécution) — un des 4 hooks issus de l'éclatement de
 * useOrderActions.ts.
 */
export function useCommandRouter(opts: {
  positions: GetPositionsResult | undefined;
  refreshMarket: () => Promise<void>;
  refreshNews: (options?: { force?: boolean }) => Promise<void>;
  /** Appelé par `setMcpUrl`/`setMcpToken` ci-dessous une fois l'écriture disque terminée — relit la
   * config et force un remount avec un nouveau CtraderClient (cf. App.tsx#reloadConfig). */
  onCredentialsChanged: () => void;
  hasMcpUrl: boolean;
  hasMcpToken: boolean;
  /** Valeur chargée depuis `~/.aurum/settings.json` (cf. settings.ts#readConfig) au montage — pas de
   * défaut codé en dur ici, App.tsx est seul responsable de résoudre "absent du fichier = activé". */
  initialAtrRefreshEnabled: boolean;
  /** Même provenance que `initialAtrRefreshEnabled` ci-dessus (settings.ts#readConfig au montage). */
  initialAtrPeriod: number;
  initialAtrTimeframe: TrendbarPeriod;
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
    onCredentialsChanged,
    hasMcpUrl,
    hasMcpToken,
    initialAtrRefreshEnabled,
    initialAtrPeriod,
    initialAtrTimeframe,
    tradeConfirm,
    modifyConfirm,
    cancelConfirm,
    positionAmendConfirm,
    closeConfirm,
  } = opts;
  const { client, symbolId } = useCtrader();
  const { setFeedback } = useFeedback();

  // Basculé par Shift+Tab (cf. useTerminalShortcuts.ts), lu par `trade` via ctx.atrMode.
  const [atrMode, setAtrMode] = useState(false);
  function toggleAtrMode() {
    setAtrMode((v) => !v);
  }

  // Réglé via `settings atrrefresh on|off` (cf. commands/settings.ts) ou lu depuis le disque au
  // lancement — contrairement à `atrMode`, celui-ci est persisté : `setAtrRefreshEnabled` met à
  // jour le state ET écrit sur disque (fusion, cf. settings.ts#writeConfig), pour que
  // `commands/settings.ts` n'ait pas besoin de connaître settings.ts.
  const [atrRefreshEnabled, setAtrRefreshEnabledState] = useState(initialAtrRefreshEnabled);
  function setAtrRefreshEnabled(enabled: boolean) {
    setAtrRefreshEnabledState(enabled);
    void fsRuntime.runPromise(writeConfig({ atrRefreshEnabled: enabled }));
  }

  // Réglés via `settings atrperiod`/`settings atrtimeframe` — même discipline state+disque que
  // `atrRefreshEnabled` ci-dessus.
  const [atrPeriod, setAtrPeriodState] = useState(initialAtrPeriod);
  function setAtrPeriod(period: number) {
    setAtrPeriodState(period);
    void fsRuntime.runPromise(writeConfig({ atrPeriod: period }));
  }
  const [atrTimeframe, setAtrTimeframeState] = useState(initialAtrTimeframe);
  function setAtrTimeframe(timeframe: TrendbarPeriod) {
    setAtrTimeframeState(timeframe);
    void fsRuntime.runPromise(writeConfig({ atrTimeframe: timeframe }));
  }

  // Contrairement à `setAtrRefreshEnabled`, pas de state local à mettre à jour : url/token changent
  // déclenchent toujours un remount complet via `onCredentialsChanged` (nouveau CtraderClient,
  // besoin d'un connect() frais) — `hasMcpUrl`/`hasMcpToken` viennent donc directement d'App.tsx,
  // pas d'un state possédé ici.
  function setMcpUrl(url: string) {
    void fsRuntime.runPromise(writeConfig({ url })).then(onCredentialsChanged);
  }
  function setMcpToken(token: string) {
    void fsRuntime.runPromise(writeConfig({ token })).then(onCredentialsChanged);
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
      atrMode,
      atrRefreshEnabled,
      setAtrRefreshEnabled,
      atrPeriod,
      atrTimeframe,
      setAtrPeriod,
      setAtrTimeframe,
      hasMcpUrl,
      hasMcpToken,
      setMcpUrl,
      setMcpToken,
      setFeedback,
      refreshMarket,
      refreshNews,
      proposeTrade: tradeConfirm.proposeTrade,
      proposeModify: modifyConfirm.proposeModify,
      proposeCancel: cancelConfirm.proposeCancel,
      proposePositionAmend: positionAmendConfirm.proposePositionAmend,
      proposeClose: closeConfirm.proposeClose,
      commands: COMMANDS,
    };
    command.run(args, ctx);
  }

  return {
    runCommand,
    atrMode,
    toggleAtrMode,
    atrRefreshEnabled,
    setAtrRefreshEnabled,
    atrPeriod,
    atrTimeframe,
    setAtrPeriod,
    setAtrTimeframe,
  };
}
