import { Effect } from "effect";
import { useState } from "react";
import { formatTradeSummary } from "../../domain/commands.ts";
import {
  conflictsWithHtfBias,
  type PreparedTrade,
  toCreateOrderParams,
} from "../../domain/trading.ts";
import { toMessage } from "../../errors.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { useFeedback } from "../context/FeedbackContext.tsx";
import type { AtrOrderTracking } from "./useAtrOrderTracking.ts";
import { h1ConfirmedBias, type TrendRow } from "./useTrend.ts";

export interface TradeConfirm {
  pendingTrade: PreparedTrade | undefined;
  /** Reçoit un `PreparedTrade` déjà calculé (cf. useCommandRouter.ts#runCommand, cas "trade") — pas
   * de parsing/préparation ici, uniquement le cycle propose → confirme/annule. */
  proposeTrade: (trade: PreparedTrade) => void;
  confirmPendingTrade: () => void;
  cancelPendingTrade: () => void;
}

/** Un des 3 hooks de confirmation issus de l'éclatement de useOrderActions.ts (cf.
 * docs/ARCHITECTURE.md §8) — possède `pendingTrade` et tout son cycle de vie, y compris
 * l'avertissement de biais H1 (dépend de `trendRows`, propre à la proposition d'un trade) et
 * l'enregistrement au suivi ATR sur succès (dépend de `atrTracking`, propre à la confirmation). */
export function useTradeConfirm(opts: {
  trendRows: TrendRow[] | undefined;
  refreshMarket: () => Promise<void>;
  atrTracking: Pick<AtrOrderTracking, "registerPendingAtrOrder">;
}): TradeConfirm {
  const { trendRows, refreshMarket, atrTracking } = opts;
  const { client, symbolId } = useCtrader();
  const { setFeedback } = useFeedback();
  const [pendingTrade, setPendingTrade] = useState<PreparedTrade>();

  function proposeTrade(trade: PreparedTrade) {
    setPendingTrade(trade);
    const biasWarning = conflictsWithHtfBias(trade.tradeSide, h1ConfirmedBias(trendRows))
      ? " — ⚠ contre le biais H1 confirmé"
      : "";
    setFeedback({ kind: "info", message: `trade calculé${biasWarning} — confirme dans la popup` });
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
        // Suivi ATR uniquement pour un ordre resté EN ATTENTE : un ordre MARKET est exécuté
        // immédiatement, son SL/TP ATR n'a besoin d'être calculé qu'une fois, déjà fait.
        if (!trade.atrTracking || trade.orderType === "MARKET") return;
        atrTracking.registerPendingAtrOrder({
          symbolId,
          side: trade.tradeSide,
          orderType: trade.orderType,
          volume: trade.volume,
          price: trade.entryPrice,
          atrMultiplier: trade.atrTracking.atrMultiplier,
          rewardRiskRatio: trade.atrTracking.rewardRiskRatio,
        });
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
