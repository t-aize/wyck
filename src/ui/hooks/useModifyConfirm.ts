import type { CtraderOrder } from "../../ctrader/book/CtraderOrder.ts";
import { toAmendOrderParams } from "../../trading/amendParams.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { usePendingAction } from "./usePendingAction.ts";

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

/** Un des 4 hooks de confirmation bâtis sur usePendingAction.ts. */
export function useModifyConfirm(opts: { refreshMarket: () => Promise<void> }): ModifyConfirm {
  const { client } = useCtrader();
  const {
    pending: pendingModify,
    propose,
    confirm: confirmPendingModify,
    cancel: cancelPendingModify,
  } = usePendingAction<PendingModify>({
    refreshMarket: opts.refreshMarket,
    proposeMessage: "modification calculée — confirme dans la popup",
    progressMessage: "modification en cours…",
    cancelMessage: "modification annulée",
    errorPrefix: "échec modification",
    run: ({ order, stopLoss, takeProfit }) =>
      client.amendOrder(toAmendOrderParams(order, { stopLoss, takeProfit })),
    successMessage: ({ order }) => `ordre ${order.orderId} modifié`,
  });

  function proposeModify(order: CtraderOrder, stopLoss?: number, takeProfit?: number) {
    propose({ order, stopLoss, takeProfit });
  }

  return { pendingModify, proposeModify, confirmPendingModify, cancelPendingModify };
}
