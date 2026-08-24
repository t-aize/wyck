import { Effect } from "effect";
import { useEffect, useRef, useState } from "react";
import type { CtraderClient } from "../../ctrader/client.ts";
import type { GetPositionsResult } from "../../ctrader/schemas.ts";
import { toAmendOrderParams } from "../../trading/amendParams.ts";
import { atrLevels, fetchAtr } from "../../trading/atr.ts";
import { readAtrTrades, removeAtrTrades } from "../../trading/atrTradeStore.ts";
import { fsRuntime } from "../../utils/effectRuntime.ts";
import { toMessage } from "../../utils/errors.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { useFeedback } from "../context/FeedbackContext.tsx";
import type { Feedback } from "../feedback.ts";
import { useInterval } from "./useInterval.ts";

export const ATR_REFRESH_MS = 60_000;
/** Cadence de relecture du store pour la colonne ATR de OrdersTable.tsx (§trackedOrderIds) — pas
 * liée à ATR_REFRESH_MS : juste assez court pour qu'un trade tout juste confirmé y apparaisse vite,
 * même cadence que le poll marché (PRICE_POLL_MS, cf. useMarketData.ts) pour rester cohérent avec
 * ce que l'utilisateur perçoit déjà comme le rythme de rafraîchissement de l'écran. */
const TRACKED_IDS_POLL_MS = 3_000;

export interface AtrAutoRefresh {
  enabled: boolean;
  /** Epoch ms de la dernière passe déclenchée (pas forcément terminée) — `undefined` avant la
   * toute première, pour l'affichage du compte à rebours dans CommandBar.tsx. */
  lastRunAt: number | undefined;
  /** Ids d'ordres suivis par atrTradeStore.ts, pour la colonne "ATR" de OrdersTable.tsx — un ordre
   * qui a depuis déclenché/été annulé disparaît naturellement (n'apparaît plus dans
   * `positions.orders`), pas besoin de croiser avec la purge de `runRefresh`. */
  trackedOrderIds: Set<number>;
}

const EMPTY_TRACKED_IDS: Set<number> = new Set();

async function runRefresh(
  client: CtraderClient,
  symbolId: number,
  positions: GetPositionsResult,
  setFeedback: (feedback: Feedback) => void,
  refreshMarket: () => Promise<void>,
): Promise<void> {
  const trades = await fsRuntime.runPromise(readAtrTrades());
  if (trades.length === 0) return;

  const orders = positions.orders;
  const stillPending = trades.filter((t) => orders.some((o) => o.orderId === t.orderId));
  const vanishedIds = trades.filter((t) => !stillPending.includes(t)).map((t) => t.orderId);
  if (vanishedIds.length > 0) {
    await fsRuntime.runPromise(removeAtrTrades(vanishedIds));
  }
  if (stillPending.length === 0) return;

  try {
    const atr = await Effect.runPromise(fetchAtr(client, symbolId));
    let amended = 0;
    for (const record of stillPending) {
      const order = orders.find((o) => o.orderId === record.orderId);
      if (!order) continue;
      const entryPrice = order.limitPrice ?? order.stopPrice;
      if (entryPrice === undefined) continue;
      const { stopLoss, takeProfit } = atrLevels(
        entryPrice,
        record.tradeSide,
        atr,
        record.rewardRiskRatio,
      );
      await Effect.runPromise(
        client.amendOrder(toAmendOrderParams(order, { stopLoss, takeProfit })),
      );
      amended++;
    }
    if (amended > 0) {
      setFeedback({
        kind: "success",
        message: `refresh ATR auto : ${amended} ordre(s) mis à jour`,
      });
      void refreshMarket();
    }
  } catch (error) {
    setFeedback({ kind: "error", message: `échec refresh ATR auto : ${toMessage(error)}` });
  }
}

/**
 * Boucle 60s qui recalcule et pousse le SL/TP des ordres ATR encore en attente (suivis par
 * `atrTradeStore.ts`, alimenté par `useTradeConfirm.ts`) — sans passer par une modale de
 * confirmation, contrairement à l'ancien `atrrefresh.ts` manuel : c'est un refresh en tâche de
 * fond, pas une action tapée. Calqué sur `useMarketData.ts` (même primitive `useInterval`, même
 * pattern de déclenchement immédiat dès que `symbolId`/`positions` sont connus).
 */
export function useAtrAutoRefresh(opts: {
  positions: GetPositionsResult | undefined;
  enabled: boolean;
  refreshMarket: () => Promise<void>;
}): AtrAutoRefresh {
  const { client, symbolId } = useCtrader();
  const { setFeedback } = useFeedback();
  const [lastRunAt, setLastRunAt] = useState<number>();
  const [trackedOrderIds, setTrackedOrderIds] = useState(EMPTY_TRACKED_IDS);

  // Relecture indépendante de la boucle 60s ci-dessous — la colonne ATR de OrdersTable.tsx doit
  // refléter un trade tout juste confirmé sans attendre le prochain vrai tick de refresh.
  useInterval(() => {
    void fsRuntime.runPromise(readAtrTrades()).then((trades) => {
      setTrackedOrderIds(new Set(trades.map((t) => t.orderId)));
    });
  }, TRACKED_IDS_POLL_MS);

  useInterval(() => {
    // `positions` pas encore connu (premier fetch de useMarketData pas encore arrivé) : ne rien
    // faire plutôt que traiter chaque trade suivi comme "disparu" et vider le store à chaque
    // démarrage (orders serait vu comme [] sinon).
    if (!opts.enabled || !symbolId || opts.positions === undefined) return;
    setLastRunAt(Date.now());
    void runRefresh(client, symbolId, opts.positions, setFeedback, opts.refreshMarket);
  }, ATR_REFRESH_MS);

  // `useInterval` déclenche son premier appel immédiat au montage, mais avant que `symbolId`/
  // `positions` soient connus (connexion + premier fetch marché sont async) — ce premier appel n'a
  // donc aucun effet (garde ci-dessus). Sans ce second effet, un ordre ATR déjà en attente d'une
  // session précédente n'aurait son premier refresh qu'au bout de 60s au lieu d'être immédiat —
  // même raisonnement que useMarketData.ts pour refreshMarket.
  const hasRunInitialRef = useRef(false);
  useEffect(() => {
    if (hasRunInitialRef.current) return;
    if (!opts.enabled || !symbolId || opts.positions === undefined) return;
    hasRunInitialRef.current = true;
    setLastRunAt(Date.now());
    void runRefresh(client, symbolId, opts.positions, setFeedback, opts.refreshMarket);
  }, [opts.enabled, symbolId, opts.positions, client, opts.refreshMarket, setFeedback]);

  return { enabled: opts.enabled, lastRunAt, trackedOrderIds };
}
