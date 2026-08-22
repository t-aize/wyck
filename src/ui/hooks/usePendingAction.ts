import { Effect } from "effect";
import { useState } from "react";
import { toMessage } from "../../utils/errors.ts";
import { useFeedback } from "../context/FeedbackContext.tsx";

export interface PendingAction<TPending> {
  pending: TPending | undefined;
  propose: (value: TPending) => void;
  confirm: () => void;
  cancel: () => void;
}

/**
 * Factorise le cycle propose → confirme (Effect.runPromise + succès/échec) → annule commun aux 4
 * hooks de confirmation "cible unique" (trade/modify/positionAmend/close, cf. leurs fichiers
 * respectifs) — ce cycle était copié-collé quasi à l'identique dans chacun (audit ui/).
 * `useCancelConfirm.ts` reste à part : son flux est un batch avec échec partiel par ordre, une
 * forme structurellement différente d'un "un seul pending, un seul résultat".
 *
 * Les chaînes de message restent explicites par appelant plutôt que templatées à partir d'un seul
 * verbe : l'accord de genre diffère selon l'action ("trade calculé" vs "modification calculée"),
 * un template générique serait fragile pour un gain marginal.
 */
export function usePendingAction<TPending>(opts: {
  refreshMarket: () => Promise<void>;
  proposeMessage: string;
  progressMessage: string;
  cancelMessage: string;
  errorPrefix: string;
  run: (pending: TPending) => Effect.Effect<unknown, unknown>;
  successMessage: (pending: TPending) => string;
}): PendingAction<TPending> {
  const {
    refreshMarket,
    proposeMessage,
    progressMessage,
    cancelMessage,
    errorPrefix,
    run,
    successMessage,
  } = opts;
  const { setFeedback } = useFeedback();
  const [pending, setPending] = useState<TPending>();

  function propose(value: TPending) {
    setPending(value);
    setFeedback({ kind: "info", message: proposeMessage });
  }

  function confirm() {
    if (pending === undefined) return;
    const value = pending;
    setPending(undefined);
    setFeedback({ kind: "info", message: progressMessage });
    void Effect.runPromise(run(value)).then(
      () => {
        setFeedback({ kind: "success", message: successMessage(value) });
        void refreshMarket();
      },
      (error) => setFeedback({ kind: "error", message: `${errorPrefix} : ${toMessage(error)}` }),
    );
  }

  function cancel() {
    setPending(undefined);
    setFeedback({ kind: "info", message: cancelMessage });
  }

  return { pending, propose, confirm, cancel };
}
