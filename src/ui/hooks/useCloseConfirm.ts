import { Effect } from "effect";
import { useState } from "react";
import type { CtraderPosition } from "../../ctrader/schemas.ts";
import { toClosePositionParams } from "../../domain/trading.ts";
import { toMessage } from "../../utils/errors.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { useFeedback } from "../context/FeedbackContext.tsx";

export interface CloseConfirm {
  pendingClose: (CtraderPosition & { id: number; volumeLots: number }) | undefined;
  proposeClose: (position: CtraderPosition & { id: number; volumeLots: number }) => void;
  confirmPendingClose: () => void;
  dismissPendingClose: () => void;
}

/** Un des hooks de confirmation issus de l'éclatement de useOrderActions.ts — cible unique (pas
 * de "close all", contrairement à useCancelConfirm.ts) : clôturer une position engage un P&L réel,
 * un mauvais coup groupé est d'un tout autre ordre de gravité qu'annuler des ordres en attente. */
export function useCloseConfirm(opts: { refreshMarket: () => Promise<void> }): CloseConfirm {
  const { refreshMarket } = opts;
  const { client } = useCtrader();
  const { setFeedback } = useFeedback();
  const [pendingClose, setPendingClose] = useState<
    CtraderPosition & { id: number; volumeLots: number }
  >();

  function proposeClose(position: CtraderPosition & { id: number; volumeLots: number }) {
    setPendingClose(position);
    setFeedback({ kind: "info", message: "clôture calculée — confirme dans la popup" });
  }

  function confirmPendingClose() {
    if (!pendingClose) return;
    const position = pendingClose;
    setPendingClose(undefined);
    setFeedback({ kind: "info", message: "clôture en cours…" });
    void Effect.runPromise(client.closePosition(toClosePositionParams(position))).then(
      () => {
        setFeedback({ kind: "success", message: `position ${position.id} clôturée` });
        void refreshMarket();
      },
      (error) => setFeedback({ kind: "error", message: `échec clôture : ${toMessage(error)}` }),
    );
  }

  function dismissPendingClose() {
    setPendingClose(undefined);
    setFeedback({ kind: "info", message: "clôture annulée" });
  }

  return { pendingClose, proposeClose, confirmPendingClose, dismissPendingClose };
}
