import { Effect } from "effect";
import { useState } from "react";
import type { CtraderOrder } from "../../ctrader/schemas.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { useFeedback } from "../context/FeedbackContext.tsx";

export interface CancelConfirm {
  pendingCancel: CtraderOrder[] | undefined;
  /** Reçoit la liste déjà résolue (cf. domain/commands.ts#resolveCancelTargets, appelé depuis
   * useCommandRouter.ts) — pas de parsing ici, uniquement le cycle propose → confirme/annule. */
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

  return { pendingCancel, proposeCancel, confirmPendingCancel, dismissPendingCancel };
}
