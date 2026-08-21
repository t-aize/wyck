import { Effect } from "effect";
import { useState } from "react";
import {
  formatTradeSummary,
  type PreparedTrade,
  toCreateOrderParams,
} from "../../domain/trading.ts";
import { toMessage } from "../../utils/errors.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { useFeedback } from "../context/FeedbackContext.tsx";

export interface TradeConfirm {
  pendingTrade: PreparedTrade | undefined;
  /** Reçoit un `PreparedTrade` déjà calculé (cf. useCommandRouter.ts#runCommand, cas "trade") — pas
   * de parsing/préparation ici, uniquement le cycle propose → confirme/annule. */
  proposeTrade: (trade: PreparedTrade) => void;
  confirmPendingTrade: () => void;
  cancelPendingTrade: () => void;
}

/** Un des 3 hooks de confirmation issus de l'éclatement de useOrderActions.ts — possède
 * `pendingTrade` et tout son cycle de vie. */
export function useTradeConfirm(opts: { refreshMarket: () => Promise<void> }): TradeConfirm {
  const { refreshMarket } = opts;
  const { client, symbolId } = useCtrader();
  const { setFeedback } = useFeedback();
  const [pendingTrade, setPendingTrade] = useState<PreparedTrade>();

  function proposeTrade(trade: PreparedTrade) {
    setPendingTrade(trade);
    setFeedback({ kind: "info", message: "trade calculé — confirme dans la popup" });
  }

  function confirmPendingTrade() {
    if (!pendingTrade || !symbolId) return;
    const trade = pendingTrade;
    const summary = formatTradeSummary(trade);
    setPendingTrade(undefined);
    setFeedback({ kind: "info", message: "envoi de l'ordre…" });
    void Effect.runPromise(client.createOrder(toCreateOrderParams(symbolId, trade))).then(
      () => {
        setFeedback({ kind: "success", message: `ordre envoyé : ${summary}` });
        void refreshMarket();
      },
      (error) => setFeedback({ kind: "error", message: `échec envoi : ${toMessage(error)}` }),
    );
  }

  function cancelPendingTrade() {
    setPendingTrade(undefined);
    setFeedback({ kind: "info", message: "trade annulé" });
  }

  return { pendingTrade, proposeTrade, confirmPendingTrade, cancelPendingTrade };
}
