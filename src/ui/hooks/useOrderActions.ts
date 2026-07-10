import { type Dispatch, type SetStateAction, useState } from "react";
import type { CtraderClient, CtraderOrder, GetPositionsResult } from "../../ctrader/client.ts";
import {
  formatTradeSummary,
  parseModifyCommand,
  parseTradeCommand,
} from "../../domain/commands.ts";
import type { PreparedTrade } from "../../domain/trading.ts";
import { prepareTrade, toCreateOrderParams } from "../../domain/trading.ts";
import { toMessage } from "../../errors.ts";
import type { Feedback } from "../components/CommandBar.tsx";

// Convention du projet : `.then` dans les handlers d'événements UI déclenchés depuis le rendu
// (ce fichier), `async`/`await` partout ailleurs (cf. les autres hooks de ce dossier).

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
  pendingCancel: CtraderOrder | undefined;
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
}): OrderActions {
  const { client, symbolId, positions, refreshMarket, refreshNews } = opts;

  const [feedback, setFeedback] = useState<Feedback>({
    kind: "info",
    message: "tapez help pour la liste des commandes",
  });
  const [pendingTrade, setPendingTrade] = useState<PreparedTrade>();
  const [pendingModify, setPendingModify] = useState<PendingModify>();
  const [pendingCancel, setPendingCancel] = useState<CtraderOrder>();

  function runCommand(raw: string) {
    const trimmed = raw.trim();
    if (!trimmed) return;
    const [commandRaw, ...args] = trimmed.split(/\s+/);
    const command = commandRaw?.toLowerCase() ?? "";

    switch (command) {
      case "help":
        setFeedback({
          kind: "info",
          message: "commandes : trade  modify  cancel  refresh  clear  help",
        });
        return;
      case "refresh":
        setFeedback({ kind: "info", message: "actualisation…" });
        void Promise.all([refreshMarket(), refreshNews({ force: true })]).then(() => {
          setFeedback({ kind: "success", message: "actualisé" });
        });
        return;
      case "clear":
        setFeedback({ kind: "info", message: "" });
        return;
      case "trade": {
        if (!symbolId) {
          setFeedback({ kind: "error", message: "pas encore connecté au serveur" });
          return;
        }
        const parsed = parseTradeCommand(args);
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
        const id = Number(args[0]);
        if (!Number.isFinite(id)) {
          setFeedback({
            kind: "error",
            message: `usage : cancel <id> — id invalide : "${args[0] ?? ""}"`,
          });
          return;
        }
        // Même limite que `modify` : seuls les ordres en attente (structure vérifiée) sont
        // annulables pour l'instant, pas les positions ouvertes (closePosition non exercé).
        const order = positions?.orders.find((o) => o.orderId === id);
        if (!order) {
          setFeedback({ kind: "error", message: `ordre en attente ${id} introuvable` });
          return;
        }
        setPendingCancel(order);
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
      action: () => client.amendOrder({ orderId: order.orderId, stopLoss, takeProfit }),
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
    const orderId = pendingCancel.orderId;
    setPendingCancel(undefined);
    runOrderAction({
      pending: "annulation en cours…",
      action: () => client.cancelOrder({ orderId }),
      success: () => `ordre ${orderId} annulé`,
      errorPrefix: "échec annulation",
      refreshAfter: true,
    });
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
