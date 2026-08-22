import { formatTradeSummary, toCreateOrderParams } from "../../trading/orderParams.ts";
import type { PreparedTrade } from "../../trading/types.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { usePendingAction } from "./usePendingAction.ts";

export interface TradeConfirm {
  pendingTrade: PreparedTrade | undefined;
  /** Reçoit un `PreparedTrade` déjà calculé (cf. useCommandRouter.ts#runCommand, cas "trade") — pas
   * de parsing/préparation ici, uniquement le cycle propose → confirme/annule. */
  proposeTrade: (trade: PreparedTrade) => void;
  confirmPendingTrade: () => void;
  cancelPendingTrade: () => void;
}

/** Un des 4 hooks de confirmation bâtis sur usePendingAction.ts — possède `pendingTrade` et tout
 * son cycle de vie. */
export function useTradeConfirm(opts: { refreshMarket: () => Promise<void> }): TradeConfirm {
  const { client, symbolId } = useCtrader();
  const {
    pending: pendingTrade,
    propose: proposeTrade,
    confirm,
    cancel: cancelPendingTrade,
  } = usePendingAction<PreparedTrade>({
    refreshMarket: opts.refreshMarket,
    proposeMessage: "trade calculé — confirme dans la popup",
    progressMessage: "envoi de l'ordre…",
    cancelMessage: "trade annulé",
    errorPrefix: "échec envoi",
    // `symbolId` garanti défini ici par la garde de confirmPendingTrade ci-dessous — usePendingAction
    // n'appelle `run` que depuis confirm(), jamais avant.
    run: (trade) => client.createOrder(toCreateOrderParams(symbolId as number, trade)),
    successMessage: (trade) => `ordre envoyé : ${formatTradeSummary(trade)}`,
  });

  function confirmPendingTrade() {
    if (!symbolId) return;
    confirm();
  }

  return { pendingTrade, proposeTrade, confirmPendingTrade, cancelPendingTrade };
}
