import { Effect, type ManagedRuntime } from "effect";
import { type Dispatch, type SetStateAction, useState } from "react";
import type { AtrSettings } from "../../config.ts";
import type {
  CtraderClient,
  CtraderClientLive,
  CtraderOrder,
  GetPositionsResult,
} from "../../ctrader/client.ts";
import {
  ATR_SETTINGS_USAGE,
  ATR_TRADE_USAGE,
  CANCEL_USAGE,
  formatTradeSummary,
  MODIFY_USAGE,
  parseAtrSettingsCommand,
  parseAtrTradeCommand,
  parseModifyCommand,
  parseRiskCommand,
  parseTradeCommand,
  RISK_USAGE,
  TRADE_USAGE,
} from "../../domain/commands.ts";
import type { PreparedTrade } from "../../domain/trading.ts";
import {
  conflictsWithHtfBias,
  prepareAtrTrade,
  prepareTrade,
  toCreateOrderParams,
} from "../../domain/trading.ts";
import { toMessage } from "../../errors.ts";
import type { Feedback } from "../components/CommandBar.tsx";
import type { AtrOrderTracking } from "./useAtrOrderTracking.ts";
import { h1ConfirmedBias, type TrendRow } from "./useTrend.ts";

// Convention du projet : `.then` dans les handlers d'événements UI déclenchés depuis le rendu
// (ce fichier), `async`/`await` partout ailleurs (cf. les autres hooks de ce dossier).

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

interface PendingModify {
  order: CtraderOrder;
  stopLoss?: number;
  takeProfit?: number;
}

export interface OrderActions {
  runCommand: (raw: string) => void;
  /** Basculé au Shift+Tab (cf. CommandBar.tsx) : `trade` ne prend alors que risque/entrée/direction,
   * SL/TP dérivés de l'ATR(14) M5 — cf. domain/trading.ts#prepareAtrTrade. */
  atrMode: boolean;
  toggleAtrMode: () => void;
  pendingTrade: PreparedTrade | undefined;
  confirmPendingTrade: () => void;
  cancelPendingTrade: () => void;
  pendingModify: PendingModify | undefined;
  confirmPendingModify: () => void;
  cancelPendingModify: () => void;
  pendingCancel: CtraderOrder[] | undefined;
  confirmPendingCancel: () => void;
  dismissPendingCancel: () => void;
}

/** Le routeur de commandes du CommandBar, et le cycle confirmation → envoi → feedback des ordres. */
export function useOrderActions(opts: {
  /** Écriture seule ici (jamais lu) — possédé par ConnectedApp (App.tsx), pas par ce hook : partagé
   * avec useAtrOrderTracking (le système de suivi doit pouvoir écrire dans la même barre de
   * feedback) et useTerminalShortcuts. Un seul état source évite la dépendance circulaire que
   * créerait ce hook s'il possédait lui-même ce state (useAtrOrderTracking a besoin de
   * setFeedback, et ses résultats sont eux-mêmes un opt de ce hook, cf. atrTracking ci-dessous). */
  setFeedback: Dispatch<SetStateAction<Feedback>>;
  client: CtraderClientLive;
  /** Requis uniquement par `prepareTrade`/`prepareAtrTrade` (injecté via CtraderClient, cf.
   * domain/trading.ts) — les autres actions de ce hook appellent `client` directement. */
  runtime: ManagedRuntime.ManagedRuntime<CtraderClient, never>;
  symbolId: number | undefined;
  positions: GetPositionsResult | undefined;
  refreshMarket: () => Promise<void>;
  refreshNews: (options?: { force?: boolean }) => Promise<void>;
  refreshTrend: () => Promise<void>;
  trendRows: TrendRow[] | undefined;
  onReconfigure: () => void;
  /** ATR le plus récent sur le timeframe configuré, échelle brute x10^5 (cf. useTrend.ts#atr) —
   * consommé par `trade` en mode ATR. */
  atrRaw: number | undefined;
  atrSettings: AtrSettings;
  onUpdateAtrSettings: (patch: Partial<AtrSettings>) => void;
  atrTracking: Pick<AtrOrderTracking, "registerPendingAtrOrder" | "untrackOrder">;
}): OrderActions {
  const {
    setFeedback,
    client,
    runtime,
    symbolId,
    positions,
    refreshMarket,
    refreshNews,
    refreshTrend,
    trendRows,
    onReconfigure,
    atrRaw,
    atrSettings,
    onUpdateAtrSettings,
    atrTracking,
  } = opts;

  // Comme defaultRiskPercent ci-dessous : basculé au Shift+Tab, jamais persisté, repart à "manuel"
  // à chaque lancement.
  const [atrMode, setAtrMode] = useState(false);
  const [pendingTrade, setPendingTrade] = useState<PreparedTrade>();
  const [pendingModify, setPendingModify] = useState<PendingModify>();
  const [pendingCancel, setPendingCancel] = useState<CtraderOrder[]>();
  // Réglé via la commande `risk`, jamais persisté : repart à zéro à chaque lancement plutôt que
  // de continuer à trader silencieusement sur un risque défini une session précédente et oublié.
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

        // Callback commun aux deux modes : même popup de confirmation, même avertissement biais H1
        // non bloquant (cf. domain/trading.ts#conflictsWithHtfBias).
        const onPrepared = (trade: PreparedTrade) => {
          setPendingTrade(trade);
          const biasWarning = conflictsWithHtfBias(trade.tradeSide, h1ConfirmedBias(trendRows))
            ? " — ⚠ contre le biais H1 confirmé"
            : "";
          setFeedback({
            kind: "info",
            message: `trade calculé${biasWarning} — confirme dans la popup`,
          });
        };
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
            .then(onPrepared, onFailed);
          return;
        }

        const parsed = parseTradeCommand(args, defaultRiskPercent);
        if (typeof parsed === "string") {
          setFeedback({ kind: "error", message: parsed });
          return;
        }
        setFeedback({ kind: "info", message: "calcul en cours…" });
        void runtime.runPromise(prepareTrade(symbolId, parsed)).then(onPrepared, onFailed);
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
        setPendingModify({ order, stopLoss: parsed.stopLoss, takeProfit: parsed.takeProfit });
        setFeedback({ kind: "info", message: "modification calculée — confirme dans la popup" });
        return;
      }
      case "cancel": {
        if (args.length === 0) {
          setFeedback({ kind: "error", message: CANCEL_USAGE });
          return;
        }
        if (args[0]?.toLowerCase() === "all") {
          const allOrders = positions?.orders ?? [];
          if (allOrders.length === 0) {
            setFeedback({ kind: "info", message: "aucun ordre en attente à annuler" });
            return;
          }
          setPendingCancel(allOrders);
          setFeedback({
            kind: "info",
            message: "annulation de tous les ordres — confirme dans la popup",
          });
          return;
        }
        const ids = args.map(Number);
        const invalid = args[ids.findIndex((id) => !Number.isFinite(id))];
        if (invalid !== undefined) {
          setFeedback({ kind: "error", message: `id invalide : "${invalid}"` });
          return;
        }
        // Même limite que `modify` : seuls les ordres en attente (structure vérifiée) sont
        // annulables pour l'instant, pas les positions ouvertes (closePosition non exercé).
        const orders = ids.map((id) => positions?.orders.find((o) => o.orderId === id));
        const missingIndex = orders.findIndex((o) => !o);
        if (missingIndex !== -1) {
          setFeedback({
            kind: "error",
            message: `ordre en attente ${ids[missingIndex]} introuvable`,
          });
          return;
        }
        setPendingCancel(orders as CtraderOrder[]);
        setFeedback({ kind: "info", message: "annulation — confirme dans la popup" });
        return;
      }
      default:
        setFeedback({
          kind: "error",
          message: `commande inconnue : ${command} — help pour la liste`,
        });
    }
  }

  /** Cycle commun aux confirmations d'ordre : feedback "en cours" → action → feedback succès/erreur. */
  function runOrderAction(actionOpts: {
    pending: string;
    action: () => Promise<unknown>;
    success: () => string;
    errorPrefix: string;
    refreshAfter?: boolean;
    onSuccess?: () => void;
  }): void {
    setFeedback({ kind: "info", message: actionOpts.pending });
    void actionOpts.action().then(
      () => {
        setFeedback({ kind: "success", message: actionOpts.success() });
        if (actionOpts.refreshAfter) void refreshMarket();
        actionOpts.onSuccess?.();
      },
      (error) =>
        setFeedback({ kind: "error", message: `${actionOpts.errorPrefix} : ${toMessage(error)}` }),
    );
  }

  function confirmPendingTrade() {
    if (!pendingTrade || !symbolId) return;
    const trade = pendingTrade;
    const summary = formatTradeSummary(trade);
    setPendingTrade(undefined);
    runOrderAction({
      pending: "envoi de l'ordre…",
      action: () => Effect.runPromise(client.createOrder(toCreateOrderParams(symbolId, trade))),
      success: () => `ordre envoyé : ${summary}`,
      errorPrefix: "échec envoi",
      // Rafraîchit tout de suite plutôt que d'attendre le prochain poll (jusqu'à 3s) — réduit la
      // latence de correspondance du suivi ATR ci-dessous ; gain latent aussi hors mode ATR.
      refreshAfter: true,
      onSuccess: () => {
        // Suivi ATR uniquement pour un ordre resté EN ATTENTE (cf. plan) : un ordre MARKET est
        // exécuté immédiatement, son SL/TP ATR n'a besoin d'être calculé qu'une fois, déjà fait.
        if (!trade.atrTracking || trade.orderType === "MARKET") return;
        atrTracking.registerPendingAtrOrder({
          symbolId,
          side: trade.tradeSide,
          orderType: trade.orderType,
          volume: trade.volume,
          price: trade.entryPrice,
          atrMultiplier: trade.atrTracking.atrMultiplier,
          rewardRiskRatio: trade.atrTracking.rewardRiskRatio,
        });
      },
    });
  }

  function cancelPendingTrade() {
    setPendingTrade(undefined);
    setFeedback({ kind: "info", message: "trade annulé" });
  }

  function confirmPendingModify() {
    if (!pendingModify) return;
    const { order, stopLoss, takeProfit } = pendingModify;
    setPendingModify(undefined);
    // Intervention manuelle sur un ordre suivi ATR : elle prime, désactive le suivi automatique
    // (no-op si l'ordre n'était pas suivi).
    atrTracking.untrackOrder(order.orderId);
    runOrderAction({
      pending: "modification en cours…",
      // cTrader remet à 0 tout champ prix non renvoyé à l'amend (limitPrice/stopPrice
      // mais aussi SL/TP) — il faut toujours resend les valeurs existantes non modifiées.
      action: () =>
        Effect.runPromise(
          client.amendOrder({
            orderId: order.orderId,
            limitPrice: order.limitPrice,
            stopPrice: order.stopPrice,
            stopLoss: stopLoss ?? order.stopLoss,
            takeProfit: takeProfit ?? order.takeProfit,
          }),
        ),
      success: () => `ordre ${order.orderId} modifié`,
      errorPrefix: "échec modification",
      refreshAfter: true,
    });
  }

  function cancelPendingModify() {
    setPendingModify(undefined);
    setFeedback({ kind: "info", message: "modification annulée" });
  }

  function confirmPendingCancel() {
    if (!pendingCancel) return;
    const orders = pendingCancel;
    setPendingCancel(undefined);
    setFeedback({ kind: "info", message: "annulation en cours…" });

    // Chaque résultat porte directement sa commande (§2.2, docs/ARCHITECTURE.md) plutôt que d'associer
    // `orders[i]`/`results[i]` par index comme le faisait le Promise.allSettled précédent — plus
    // fragile si jamais l'un des deux tableaux venait à diverger.
    const cancelAll = Effect.forEach(
      orders,
      (order) =>
        client.cancelOrder({ orderId: order.orderId }).pipe(
          Effect.as({ order, ok: true as const }),
          Effect.catchAll(() => Effect.succeed({ order, ok: false as const })),
        ),
      { concurrency: "unbounded" },
    );

    void Effect.runPromise(cancelAll).then((results) => {
      void refreshMarket();
      const failed = results.filter((r) => !r.ok).map((r) => r.order);
      if (failed.length === 0) {
        setFeedback({
          kind: "success",
          message: `ordre${orders.length > 1 ? "s" : ""} ${orders.map((o) => o.orderId).join(", ")} annulé${orders.length > 1 ? "s" : ""}`,
        });
      } else {
        setFeedback({
          kind: "error",
          message: `échec annulation : ${failed.map((o) => o.orderId).join(", ")}`,
        });
      }
    });
  }

  function dismissPendingCancel() {
    setPendingCancel(undefined);
    setFeedback({ kind: "info", message: "annulation abandonnée" });
  }

  function toggleAtrMode() {
    setAtrMode((current) => !current);
  }

  return {
    runCommand,
    atrMode,
    toggleAtrMode,
    pendingTrade,
    confirmPendingTrade,
    cancelPendingTrade,
    pendingModify,
    confirmPendingModify,
    cancelPendingModify,
    pendingCancel,
    confirmPendingCancel,
    dismissPendingCancel,
  };
}
