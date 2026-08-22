import type { ClosablePosition } from "../../ctrader/schemas.ts";
import { toClosePositionParams } from "../../trading/amendParams.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { usePendingAction } from "./usePendingAction.ts";

export interface CloseConfirm {
  pendingClose: ClosablePosition | undefined;
  proposeClose: (position: ClosablePosition) => void;
  confirmPendingClose: () => void;
  dismissPendingClose: () => void;
}

/** Un des hooks de confirmation bâtis sur usePendingAction.ts — cible unique (pas de "close all",
 * contrairement à useCancelConfirm.ts) : clôturer une position engage un P&L réel, un mauvais coup
 * groupé est d'un tout autre ordre de gravité qu'annuler des ordres en attente. */
export function useCloseConfirm(opts: { refreshMarket: () => Promise<void> }): CloseConfirm {
  const { client } = useCtrader();
  const {
    pending: pendingClose,
    propose: proposeClose,
    confirm: confirmPendingClose,
    cancel: dismissPendingClose,
  } = usePendingAction<ClosablePosition>({
    refreshMarket: opts.refreshMarket,
    proposeMessage: "clôture calculée — confirme dans la popup",
    progressMessage: "clôture en cours…",
    cancelMessage: "clôture annulée",
    errorPrefix: "échec clôture",
    run: (position) => client.closePosition(toClosePositionParams(position)),
    successMessage: (position) => `position ${position.id} clôturée`,
  });

  return { pendingClose, proposeClose, confirmPendingClose, dismissPendingClose };
}
