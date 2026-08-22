import { Effect } from "effect";
import { useState } from "react";
import type { CtraderPosition } from "../../ctrader/schemas.ts";
import { toAmendPositionParams } from "../../domain/trading.ts";
import { toMessage } from "../../utils/errors.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { useFeedback } from "../context/FeedbackContext.tsx";

export interface PendingPositionAmend {
  position: CtraderPosition & { id: number };
  stopLoss?: number;
  takeProfit?: number;
}

export interface PositionAmendConfirm {
  pendingPositionAmend: PendingPositionAmend | undefined;
  proposePositionAmend: (
    position: CtraderPosition & { id: number },
    stopLoss?: number,
    takeProfit?: number,
  ) => void;
  confirmPendingPositionAmend: () => void;
  cancelPendingPositionAmend: () => void;
}

/** Un des hooks de confirmation issus de l'éclatement de useOrderActions.ts — pendant de
 * useModifyConfirm.ts, pour les positions ouvertes plutôt que les ordres en attente. */
export function usePositionAmendConfirm(opts: {
  refreshMarket: () => Promise<void>;
}): PositionAmendConfirm {
  const { refreshMarket } = opts;
  const { client } = useCtrader();
  const { setFeedback } = useFeedback();
  const [pendingPositionAmend, setPendingPositionAmend] = useState<PendingPositionAmend>();

  function proposePositionAmend(
    position: CtraderPosition & { id: number },
    stopLoss?: number,
    takeProfit?: number,
  ) {
    setPendingPositionAmend({ position, stopLoss, takeProfit });
    setFeedback({ kind: "info", message: "modification calculée — confirme dans la popup" });
  }

  function confirmPendingPositionAmend() {
    if (!pendingPositionAmend) return;
    const { position, stopLoss, takeProfit } = pendingPositionAmend;
    setPendingPositionAmend(undefined);
    setFeedback({ kind: "info", message: "modification en cours…" });
    void Effect.runPromise(
      client.amendPosition(toAmendPositionParams(position, { stopLoss, takeProfit })),
    ).then(
      () => {
        setFeedback({ kind: "success", message: `position ${position.id} modifiée` });
        void refreshMarket();
      },
      (error) =>
        setFeedback({ kind: "error", message: `échec modification : ${toMessage(error)}` }),
    );
  }

  function cancelPendingPositionAmend() {
    setPendingPositionAmend(undefined);
    setFeedback({ kind: "info", message: "modification annulée" });
  }

  return {
    pendingPositionAmend,
    proposePositionAmend,
    confirmPendingPositionAmend,
    cancelPendingPositionAmend,
  };
}
