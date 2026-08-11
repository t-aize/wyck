import { useState } from "react";
import type { AtrSettings } from "../../config.ts";
import type { GetPositionsResult } from "../../ctrader/client.ts";
import {
  ATR_SETTINGS_USAGE,
  ATR_TRADE_USAGE,
  CANCEL_USAGE,
  MODIFY_USAGE,
  parseAtrSettingsCommand,
  parseAtrTradeCommand,
  parseModifyCommand,
  parseRiskCommand,
  parseTradeCommand,
  RISK_USAGE,
  resolveCancelTargets,
  TRADE_USAGE,
} from "../../domain/commands.ts";
import { prepareAtrTrade, prepareTrade } from "../../domain/trading.ts";
import { toMessage } from "../../errors.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { useFeedback } from "../context/FeedbackContext.tsx";
import type { CancelConfirm } from "./useCancelConfirm.ts";
import type { ModifyConfirm } from "./useModifyConfirm.ts";
import type { TradeConfirm } from "./useTradeConfirm.ts";

const COMMAND_LIST = "trade  modify  cancel  risk  atr  settings  refresh  clear  help";

/**
 * Détail affiché par `help <commande>` — réutilise les mêmes chaînes d'usage que les erreurs de
 * parsing. `trade` dépend de `atrMode` (basculé au Shift+Tab, cf. CommandBar.tsx) : `help trade`
 * montre toujours l'usage du mode réellement actif, pas systématiquement le mode manuel.
 */
function commandHelp(atrMode: boolean): Record<string, string> {
  return {
    trade: atrMode ? ATR_TRADE_USAGE : TRADE_USAGE,
    modify: MODIFY_USAGE,
    cancel: CANCEL_USAGE,
    risk: RISK_USAGE,
    atr: ATR_SETTINGS_USAGE,
    settings: "settings — reconfigure l'URL/le token MCP",
    refresh:
      "refresh — force une actualisation immédiate du marché, du calendrier et de la tendance",
    clear: "clear — efface le message de feedback",
    help: "help [commande] — liste les commandes, ou détaille l'usage d'une commande précise",
  };
}

export interface CommandRouter {
  runCommand: (raw: string) => void;
  /** Basculé au Shift+Tab (cf. CommandBar.tsx) : `trade` ne prend alors que risque/entrée/direction,
   * SL/TP dérivés de l'ATR(14) M5 — cf. domain/trading.ts#prepareAtrTrade. */
  atrMode: boolean;
  toggleAtrMode: () => void;
}

/** Parsing + dispatch des commandes du CommandBar — un des 4 hooks issus de l'éclatement de
 * useOrderActions.ts (cf. docs/ARCHITECTURE.md §8). Sur succès de `trade`/`modify`/`cancel`, délègue
 * au hook de confirmation correspondant plutôt que de posséder lui-même cet état. */
export function useCommandRouter(opts: {
  positions: GetPositionsResult | undefined;
  refreshMarket: () => Promise<void>;
  refreshNews: (options?: { force?: boolean }) => Promise<void>;
  refreshTrend: () => Promise<void>;
  onReconfigure: () => void;
  /** ATR le plus récent sur le timeframe configuré, échelle brute x10^5 (cf. useTrend.ts#atr) —
   * consommé par `trade` en mode ATR. */
  atrRaw: number | undefined;
  atrSettings: AtrSettings;
  onUpdateAtrSettings: (patch: Partial<AtrSettings>) => void;
  tradeConfirm: Pick<TradeConfirm, "proposeTrade">;
  modifyConfirm: Pick<ModifyConfirm, "proposeModify">;
  cancelConfirm: Pick<CancelConfirm, "proposeCancel">;
}): CommandRouter {
  const {
    positions,
    refreshMarket,
    refreshNews,
    refreshTrend,
    onReconfigure,
    atrRaw,
    atrSettings,
    onUpdateAtrSettings,
    tradeConfirm,
    modifyConfirm,
    cancelConfirm,
  } = opts;
  const { runtime, symbolId } = useCtrader();
  const { setFeedback } = useFeedback();

  // Basculé au Shift+Tab, jamais persisté, repart à "manuel" à chaque lancement.
  const [atrMode, setAtrMode] = useState(false);
  // Réglé via la commande `risk`, jamais persisté : repart à zéro à chaque lancement plutôt que de
  // continuer à trader silencieusement sur un risque défini une session précédente et oublié.
  const [defaultRiskPercent, setDefaultRiskPercent] = useState<number>();

  function runCommand(raw: string) {
    const trimmed = raw.trim();
    if (!trimmed) return;
    const [commandRaw, ...args] = trimmed.split(/\s+/);
    const command = commandRaw?.toLowerCase() ?? "";

    switch (command) {
      case "help": {
        const target = args[0]?.toLowerCase();
        const modeNote = atrMode ? " · mode ATR actif (Shift+Tab pour basculer)" : "";
        if (!target) {
          setFeedback({ kind: "info", message: `commandes : ${COMMAND_LIST}${modeNote}` });
          return;
        }
        const detail = commandHelp(atrMode)[target];
        setFeedback(
          detail
            ? { kind: "info", message: detail }
            : { kind: "error", message: `commande inconnue : ${target} — ${COMMAND_LIST}` },
        );
        return;
      }
      case "settings":
        onReconfigure();
        return;
      case "refresh":
        setFeedback({ kind: "info", message: "actualisation…" });
        void Promise.all([refreshMarket(), refreshNews({ force: true }), refreshTrend()]).then(
          () => {
            setFeedback({ kind: "success", message: "actualisé" });
          },
        );
        return;
      case "clear":
        setFeedback({ kind: "info", message: "" });
        return;
      case "risk": {
        if (args.length === 0) {
          setFeedback({
            kind: "info",
            message:
              defaultRiskPercent === undefined
                ? `aucun risque par défaut — ${RISK_USAGE}`
                : `risque par défaut : ${defaultRiskPercent}%`,
          });
          return;
        }
        const parsedRisk = parseRiskCommand(args);
        if (typeof parsedRisk === "string") {
          setFeedback({ kind: "error", message: parsedRisk });
          return;
        }
        setDefaultRiskPercent(parsedRisk);
        setFeedback({
          kind: "success",
          message: `risque par défaut réglé à ${parsedRisk}% pour cette session`,
        });
        return;
      }
      case "atr": {
        const parsed = parseAtrSettingsCommand(args);
        if (typeof parsed === "string") {
          setFeedback({ kind: "error", message: parsed });
          return;
        }
        if (Object.keys(parsed).length === 0) {
          setFeedback({
            kind: "info",
            message:
              `RR ${atrSettings.rewardRiskRatio} · multiplicateur ATR ${atrSettings.atrMultiplier} ` +
              `· période ${atrSettings.atrPeriod} · timeframe ${atrSettings.atrTimeframe} — ${ATR_SETTINGS_USAGE}`,
          });
          return;
        }
        // Persisté (config.json), contrairement à `risk`/`atrMode` — cf. commentaire de tête de
        // config.ts#DEFAULT_ATR_SETTINGS.
        onUpdateAtrSettings(parsed);
        setFeedback({ kind: "success", message: "réglages ATR mis à jour" });
        return;
      }
      case "trade": {
        if (!symbolId) {
          setFeedback({ kind: "error", message: "pas encore connecté au serveur" });
          return;
        }
        const onFailed = (error: unknown) =>
          setFeedback({ kind: "error", message: toMessage(error) });

        if (atrMode) {
          const parsed = parseAtrTradeCommand(args, defaultRiskPercent);
          if (typeof parsed === "string") {
            setFeedback({ kind: "error", message: parsed });
            return;
          }
          setFeedback({ kind: "info", message: "calcul ATR en cours…" });
          void runtime
            .runPromise(
              prepareAtrTrade(symbolId, parsed, {
                rawValue: atrRaw,
                multiplier: atrSettings.atrMultiplier,
                rewardRiskRatio: atrSettings.rewardRiskRatio,
                period: atrSettings.atrPeriod,
                timeframe: atrSettings.atrTimeframe,
              }),
            )
            .then(tradeConfirm.proposeTrade, onFailed);
          return;
        }

        const parsed = parseTradeCommand(args, defaultRiskPercent);
        if (typeof parsed === "string") {
          setFeedback({ kind: "error", message: parsed });
          return;
        }
        setFeedback({ kind: "info", message: "calcul en cours…" });
        void runtime
          .runPromise(prepareTrade(symbolId, parsed))
          .then(tradeConfirm.proposeTrade, onFailed);
        return;
      }
      case "modify": {
        const parsed = parseModifyCommand(args);
        if (typeof parsed === "string") {
          setFeedback({ kind: "error", message: parsed });
          return;
        }
        // Seuls les ordres en attente ont une structure vérifiée (CtraderOrder) — CtraderPosition
        // reste non vérifié (aucune position réelle observée), donc `modify` ne cible que les
        // ordres pour l'instant. Cf. commentaire équivalent dans ctrader/mappers.ts.
        const order = positions?.orders.find((o) => o.orderId === parsed.id);
        if (!order) {
          setFeedback({
            kind: "error",
            message: `ordre en attente ${parsed.id} introuvable`,
          });
          return;
        }
        modifyConfirm.proposeModify(order, parsed.stopLoss, parsed.takeProfit);
        return;
      }
      case "cancel": {
        const resolved = resolveCancelTargets(args, positions?.orders ?? []);
        if (resolved.kind === "rejected") {
          setFeedback({ kind: resolved.level, message: resolved.message });
          return;
        }
        cancelConfirm.proposeCancel(resolved.orders);
        return;
      }
      default:
        setFeedback({
          kind: "error",
          message: `commande inconnue : ${command} — help pour la liste`,
        });
    }
  }

  function toggleAtrMode() {
    setAtrMode((current) => !current);
  }

  return { runCommand, atrMode, toggleAtrMode };
}
