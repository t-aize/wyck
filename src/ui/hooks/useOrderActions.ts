import { type Dispatch, type SetStateAction, useState } from "react";
import { writeFredApiKey } from "../../config.ts";
import type { CtraderClient, CtraderOrder, GetPositionsResult } from "../../ctrader/client.ts";
import {
  CANCEL_USAGE,
  formatTradeSummary,
  MODIFY_USAGE,
  parseModifyCommand,
  parseRiskCommand,
  parseTradeCommand,
  RISK_USAGE,
  TRADE_USAGE,
} from "../../domain/commands.ts";
import type { PreparedTrade } from "../../domain/trading.ts";
import { prepareTrade, toCreateOrderParams } from "../../domain/trading.ts";
import { toMessage } from "../../errors.ts";
import type { Feedback } from "../components/CommandBar.tsx";

// Convention du projet : `.then` dans les handlers d'événements UI déclenchés depuis le rendu
// (ce fichier), `async`/`await` partout ailleurs (cf. les autres hooks de ce dossier).

const FRED_USAGE =
  "usage : fred <clé api>  (clé gratuite : https://fred.stlouisfed.org/docs/api/api_key.html)";

const COMMAND_LIST = "trade  modify  cancel  risk  fred  settings  refresh  clear  help";

/** Détail affiché par `help <commande>` — réutilise les mêmes chaînes d'usage que les erreurs de parsing. */
const COMMAND_HELP: Record<string, string> = {
  trade: TRADE_USAGE,
  modify: MODIFY_USAGE,
  cancel: CANCEL_USAGE,
  risk: RISK_USAGE,
  fred: FRED_USAGE,
  settings: "settings — reconfigure l'URL/le token du serveur MCP",
  refresh:
    "refresh — force une actualisation immédiate du marché, du calendrier, de la structure et du contexte macro",
  clear: "clear — efface le message de feedback",
  help: "help [commande] — liste les commandes, ou détaille l'usage d'une commande précise",
};

interface PendingModify {
  order: CtraderOrder;
  stopLoss?: number;
  takeProfit?: number;
}

export interface OrderActions {
  feedback: Feedback;
  /** Exposé pour useTerminalShortcuts (copie OSC 52, confirmation Ctrl+C) : même état partagé. */
  setFeedback: Dispatch<SetStateAction<Feedback>>;
  runCommand: (raw: string) => void;
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
  client: CtraderClient;
  symbolId: number | undefined;
  positions: GetPositionsResult | undefined;
  refreshMarket: () => Promise<void>;
  refreshNews: (options?: { force?: boolean }) => Promise<void>;
  refreshStructure: () => Promise<void>;
  refreshMacro: (options?: { force?: boolean }) => Promise<void>;
  fredApiKey: string | undefined;
  setFredApiKey: (key: string) => void;
  onReconfigure: () => void;
}): OrderActions {
  const {
    client,
    symbolId,
    positions,
    refreshMarket,
    refreshNews,
    refreshStructure,
    refreshMacro,
    fredApiKey,
    setFredApiKey,
    onReconfigure,
  } = opts;

  const [feedback, setFeedback] = useState<Feedback>({
    kind: "info",
    message: "tapez help pour la liste des commandes",
  });
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
        if (!target) {
          setFeedback({ kind: "info", message: `commandes : ${COMMAND_LIST}` });
          return;
        }
        const detail = COMMAND_HELP[target];
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
        void Promise.all([
          refreshMarket(),
          refreshNews({ force: true }),
          refreshStructure(),
          refreshMacro({ force: true }),
        ]).then(() => {
          setFeedback({ kind: "success", message: "actualisé" });
        });
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
      case "fred": {
        if (args.length === 0) {
          setFeedback({
            kind: "info",
            message:
              fredApiKey === undefined
                ? `aucune clé FRED configurée — ${FRED_USAGE}`
                : "clé FRED configurée",
          });
          return;
        }
        const key = args[0]!;
        try {
          writeFredApiKey(key);
          setFredApiKey(key);
          setFeedback({ kind: "success", message: "clé FRED enregistrée" });
          void refreshMacro({ force: true });
        } catch (error) {
          setFeedback({ kind: "error", message: toMessage(error) });
        }
        return;
      }
      case "trade": {
        if (!symbolId) {
          setFeedback({ kind: "error", message: "pas encore connecté au serveur" });
          return;
        }
        const parsed = parseTradeCommand(args, defaultRiskPercent);
        if (typeof parsed === "string") {
          setFeedback({ kind: "error", message: parsed });
          return;
        }
        setFeedback({ kind: "info", message: "calcul en cours…" });
        void prepareTrade(client, symbolId, parsed).then(
          (trade) => {
            setPendingTrade(trade);
            setFeedback({ kind: "info", message: "trade calculé — confirme dans la popup" });
          },
          (error) => setFeedback({ kind: "error", message: toMessage(error) }),
        );
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
  }): void {
    setFeedback({ kind: "info", message: actionOpts.pending });
    void actionOpts.action().then(
      () => {
        setFeedback({ kind: "success", message: actionOpts.success() });
        if (actionOpts.refreshAfter) void refreshMarket();
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
      action: () => client.createOrder(toCreateOrderParams(symbolId, trade)),
      success: () => `ordre envoyé : ${summary}`,
      errorPrefix: "échec envoi",
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
    runOrderAction({
      pending: "modification en cours…",
      // cTrader remet à 0 tout champ prix non renvoyé à l'amend (limitPrice/stopPrice
      // mais aussi SL/TP) — il faut toujours resend les valeurs existantes non modifiées.
      action: () =>
        client.amendOrder({
          orderId: order.orderId,
          limitPrice: order.limitPrice,
          stopPrice: order.stopPrice,
          stopLoss: stopLoss ?? order.stopLoss,
          takeProfit: takeProfit ?? order.takeProfit,
        }),
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
    void Promise.allSettled(orders.map((o) => client.cancelOrder({ orderId: o.orderId }))).then(
      (results) => {
        void refreshMarket();
        const failed = orders.filter((_, i) => results[i]?.status === "rejected");
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
      },
    );
  }

  function dismissPendingCancel() {
    setPendingCancel(undefined);
    setFeedback({ kind: "info", message: "annulation abandonnée" });
  }

  return {
    feedback,
    setFeedback,
    runCommand,
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
