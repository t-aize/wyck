import { Effect } from "effect";
import { useState } from "react";
import type { CtraderOrder } from "../../ctrader/schemas.ts";
import { toAmendOrderParams } from "../../domain/trading.ts";
import { toMessage } from "../../utils/errors.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { useFeedback } from "../context/FeedbackContext.tsx";

export interface PendingModify {
  order: CtraderOrder;
  stopLoss?: number;
  takeProfit?: number;
}

export interface ModifyConfirm {
  pendingModify: PendingModify | undefined;
  proposeModify: (order: CtraderOrder, stopLoss?: number, takeProfit?: number) => void;
  confirmPendingModify: () => void;
  cancelPendingModify: () => void;
}

/** Un des 3 hooks de confirmation issus de l'éclatement de useOrderActions.ts. */
export function useModifyConfirm(opts: { refreshMarket: () => Promise<void> }): ModifyConfirm {
  const { refreshMarket } = opts;
  const { client } = useCtrader();
  const { setFeedback } = useFeedback();
  const [pendingModify, setPendingModify] = useState<PendingModify>();

  function proposeModify(order: CtraderOrder, stopLoss?: number, takeProfit?: number) {
    setPendingModify({ order, stopLoss, takeProfit });
    setFeedback({ kind: "info", message: "modification calculée — confirme dans la popup" });
  }

  function confirmPendingModify() {
    if (!pendingModify) return;
    const { order, stopLoss, takeProfit } = pendingModify;
    setPendingModify(undefined);
    setFeedback({ kind: "info", message: "modification en cours…" });
    void Effect.runPromise(
      client.amendOrder(toAmendOrderParams(order, { stopLoss, takeProfit })),
    ).then(
      () => {
        setFeedback({ kind: "success", message: `ordre ${order.orderId} modifié` });
        void refreshMarket();
      },
      (error) =>
        setFeedback({ kind: "error", message: `échec modification : ${toMessage(error)}` }),
    );
  }

  function cancelPendingModify() {
    setPendingModify(undefined);
    setFeedback({ kind: "info", message: "modification annulée" });
  }

  return { pendingModify, proposeModify, confirmPendingModify, cancelPendingModify };
}
