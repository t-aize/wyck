import type { AmendablePosition } from "@aurum/ctrader";
import { toAmendPositionParams } from "../../trading/amendParams.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { usePendingAction } from "./usePendingAction.ts";

export interface PendingPositionAmend {
  position: AmendablePosition;
  stopLoss?: number;
  takeProfit?: number;
}

export interface PositionAmendConfirm {
  pendingPositionAmend: PendingPositionAmend | undefined;
  proposePositionAmend: (
    position: AmendablePosition,
    stopLoss?: number,
    takeProfit?: number,
  ) => void;
  confirmPendingPositionAmend: () => void;
  cancelPendingPositionAmend: () => void;
}

/** Un des hooks de confirmation bâtis sur usePendingAction.ts — pendant de useModifyConfirm.ts,
 * pour les positions ouvertes plutôt que les ordres en attente. */
export function usePositionAmendConfirm(opts: {
  refreshMarket: () => Promise<void>;
}): PositionAmendConfirm {
  const { client } = useCtrader();
  const {
    pending: pendingPositionAmend,
    propose,
    confirm: confirmPendingPositionAmend,
    cancel: cancelPendingPositionAmend,
  } = usePendingAction<PendingPositionAmend>({
    refreshMarket: opts.refreshMarket,
    proposeMessage: "modification calculée — confirme dans la popup",
    progressMessage: "modification en cours…",
    cancelMessage: "modification annulée",
    errorPrefix: "échec modification",
    run: ({ position, stopLoss, takeProfit }) =>
      client.amendPosition(toAmendPositionParams(position, { stopLoss, takeProfit })),
    successMessage: ({ position }) => `position ${position.id} modifiée`,
  });

  function proposePositionAmend(
    position: AmendablePosition,
    stopLoss?: number,
    takeProfit?: number,
  ) {
    propose({ position, stopLoss, takeProfit });
  }

  return {
    pendingPositionAmend,
    proposePositionAmend,
    confirmPendingPositionAmend,
    cancelPendingPositionAmend,
  };
}
