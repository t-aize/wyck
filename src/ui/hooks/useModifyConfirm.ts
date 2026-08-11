import { Effect } from "effect";
import { useState } from "react";
import type { CtraderOrder } from "../../ctrader/client.ts";
import { toMessage } from "../../errors.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { useFeedback } from "../context/FeedbackContext.tsx";
import type { AtrOrderTracking } from "./useAtrOrderTracking.ts";

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

/** Un des 3 hooks de confirmation issus de l'éclatement de useOrderActions.ts (cf.
 * docs/ARCHITECTURE.md §8). Une intervention manuelle sur un ordre suivi ATR désactive son suivi
 * automatique dès la confirmation (l'intervention manuelle prime, cf. useAtrOrderTracking.ts). */
export function useModifyConfirm(opts: {
  refreshMarket: () => Promise<void>;
  atrTracking: Pick<AtrOrderTracking, "untrackOrder">;
}): ModifyConfirm {
  const { refreshMarket, atrTracking } = opts;
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
    atrTracking.untrackOrder(order.orderId);
    setFeedback({ kind: "info", message: "modification en cours…" });
    // cTrader remet à 0 tout champ prix non renvoyé à l'amend (limitPrice/stopPrice mais aussi
    // SL/TP) — il faut toujours resend les valeurs existantes non modifiées.
    void Effect.runPromise(
      client.amendOrder({
        orderId: order.orderId,
        limitPrice: order.limitPrice,
        stopPrice: order.stopPrice,
        stopLoss: stopLoss ?? order.stopLoss,
        takeProfit: takeProfit ?? order.takeProfit,
      }),
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
