import { Effect } from "effect";
import { useState } from "react";
import type { CtraderOrder } from "../../ctrader/schemas.ts";
import { removeAtrTrades } from "../../trading/atrTradeStore.ts";
import { fsRuntime } from "../../utils/effectRuntime.ts";
import { toMessage } from "../../utils/errors.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { useFeedback } from "../context/FeedbackContext.tsx";

export interface CancelConfirm {
  pendingCancel: CtraderOrder[] | undefined;
  /** Reçoit la liste déjà résolue (cf. commands/close.ts, qui fusionne l'ancienne commande `cancel`
   * — appelé via useCommandRouter.ts) — pas de parsing ici, uniquement le cycle propose →
   * confirme/annule. */
  proposeCancel: (orders: CtraderOrder[]) => void;
  confirmPendingCancel: () => void;
  dismissPendingCancel: () => void;
}

/** Un des 3 hooks de confirmation issus de l'éclatement de useOrderActions.ts. */
export function useCancelConfirm(opts: { refreshMarket: () => Promise<void> }): CancelConfirm {
  const { refreshMarket } = opts;
  const { client } = useCtrader();
  const { setFeedback } = useFeedback();
  const [pendingCancel, setPendingCancel] = useState<CtraderOrder[]>();

  function proposeCancel(orders: CtraderOrder[]) {
    setPendingCancel(orders);
    setFeedback({
      kind: "info",
      message: `annulation de ${orders.length} ordre${orders.length > 1 ? "s" : ""} — confirme dans la popup`,
    });
  }

  function confirmPendingCancel() {
    if (!pendingCancel) return;
    const orders = pendingCancel;
    setPendingCancel(undefined);
    setFeedback({ kind: "info", message: "annulation en cours…" });

    // Chaque résultat porte directement sa commande plutôt que
    // d'associer orders[i]/results[i] par index — plus robuste si les deux tableaux divergeaient.
    const cancelAll = Effect.forEach(
      orders,
      (order) =>
        client.cancelOrder({ orderId: order.orderId }).pipe(
          Effect.as({ order, ok: true as const }),
          Effect.catchAll((error) => Effect.succeed({ order, ok: false as const, error })),
        ),
      { concurrency: "unbounded" },
    );

    void Effect.runPromise(cancelAll).then((results) => {
      void refreshMarket();
      // Purge immédiate du suivi ATR (cf. atrTradeStore.ts) pour les ordres réellement annulés —
      // sans ça, un ordre suivi ne disparaissait de `atr-trades.json` qu'à la prochaine passe de
      // useAtrAutoRefresh.ts (jusqu'à 5 min, cf. ATR_REFRESH_MS), pas au moment de l'annulation.
      // No-op silencieux si aucun n'était suivi (cf. removeAtrTrades).
      const cancelledIds = results.filter((r) => r.ok).map((r) => r.order.orderId);
      if (cancelledIds.length > 0) {
        void fsRuntime.runPromise(removeAtrTrades(cancelledIds));
      }
      const failed = results.filter((r): r is typeof r & { ok: false } => !r.ok);
      if (failed.length === 0) {
        setFeedback({
          kind: "success",
          message: `ordre${orders.length > 1 ? "s" : ""} ${orders.map((o) => o.orderId).join(", ")} annulé${orders.length > 1 ? "s" : ""}`,
        });
      } else {
        setFeedback({
          kind: "error",
          message: `échec annulation : ${failed.map((r) => `${r.order.orderId} (${toMessage(r.error)})`).join(", ")}`,
        });
      }
    });
  }

  function dismissPendingCancel() {
    setPendingCancel(undefined);
    setFeedback({ kind: "info", message: "annulation abandonnée" });
  }

  return { pendingCancel, proposeCancel, confirmPendingCancel, dismissPendingCancel };
}
