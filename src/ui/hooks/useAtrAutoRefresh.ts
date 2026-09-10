import {
  type CtraderClient,
  type GetPositionsResult,
  TRENDBAR_PERIOD_MS,
  type TrendbarPeriod,
} from "@aurum/ctrader";
import { Effect } from "effect";
import { useEffect, useRef, useState } from "react";
import { toAmendOrderParams } from "../../trading/amendParams.ts";
import { atrLevels, fetchAtr } from "../../trading/atr.ts";
import { readAtrTrades, removeAtrTrades } from "../../trading/atrTradeStore.ts";
import { computeVolume } from "../../trading/risk.ts";
import { fsRuntime } from "../../utils/effectRuntime.ts";
import { toMessage } from "../../utils/errors.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { useFeedback } from "../context/FeedbackContext.tsx";
import type { Feedback } from "../feedback.ts";
import { useInterval } from "./useInterval.ts";

/** La valeur ATR ne change réellement qu'à chaque clôture de bougie du timeframe configuré (cf.
 * atr.ts#fetchAtr, `settings atrtimeframe`), pas en continu — refresh cadencé sur cette clôture
 * plutôt que sur un intervalle fixe, pour ne pas refetch la même valeur pour rien entre deux
 * clôtures. */
export function atrRefreshMs(timeframe: TrendbarPeriod): number {
  return TRENDBAR_PERIOD_MS[timeframe];
}

/** Prochaine clôture de bougie (epoch ms) pour un intervalle donné (cf. `atrRefreshMs`), alignée sur
 * l'horloge murale plutôt que sur l'instant de lancement de l'app — `Date.now()` est déjà en epoch
 * UTC, donc un multiple de `intervalMs` tombe directement sur une vraie borne de bougie (`:00`/`:05`/
 * `:10`… pour M5), aucune conversion de fuseau nécessaire. Sans cet alignement, fermer/rouvrir le
 * terminal repartait à l'intervalle plein à chaque fois au lieu de reprendre "3 minutes" si on est à
 * 13h02 et que la bougie clôture à 13h05. Exportée pour qu'App.tsx calcule le compte à rebours
 * affiché directement depuis `now` (useClock), sans avoir besoin d'un état dédié dans ce hook. */
export function nextAtrBoundaryMs(now: number, intervalMs: number): number {
  return Math.ceil(now / intervalMs) * intervalMs;
}

/** Cadence de relecture du store pour la colonne ATR de OrdersTable.tsx (§trackedOrderIds) — pas
 * liée à ATR_REFRESH_MS : juste assez court pour qu'un trade tout juste confirmé y apparaisse vite,
 * même cadence que le poll marché (PRICE_POLL_MS, cf. useMarketData.ts) pour rester cohérent avec
 * ce que l'utilisateur perçoit déjà comme le rythme de rafraîchissement de l'écran. */
const TRACKED_IDS_POLL_MS = 3_000;

export interface AtrAutoRefresh {
  enabled: boolean;
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
  atrPeriod: number,
  atrTimeframe: TrendbarPeriod,
  lotSize: number,
  digits: number,
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
    const [atr, { equity, moneyDigits }] = await Promise.all([
      Effect.runPromise(fetchAtr(client, symbolId, { period: atrPeriod, timeframe: atrTimeframe })),
      Effect.runPromise(client.getBalance()),
    ]);
    let amended = 0;
    let volumeUnchanged = 0;
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
        digits,
      );

      // Le volume est recalculé à chaque passe, pas juste figé à la prise du trade : si l'ATR
      // s'écarte, le SL (= distance ATR) s'écarte aussi — sans réajuster le volume en conséquence,
      // le risque réel dériverait bien au-delà du risque% demandé (ex. 0.1% visé, 20$ de distance de
      // stop obtenus au lieu de 10$ si l'ATR double entretemps). `atr` sert de distance de stop, même
      // formule qu'à la création (cf. prepareAtr.ts).
      const riskAmount = (equity / 10 ** moneyDigits) * (record.riskPercent / 100);
      const volumeResult = await Effect.runPromise(
        Effect.either(computeVolume(riskAmount, atr, lotSize)),
      );
      // Volume sous le minimum du compte (ATR devenu trop large pour ce risque%) : on ne peut pas
      // resynchroniser le risque à la baisse, mais on met quand même le SL/TP à jour plutôt que de
      // tout bloquer — `toAmendOrderParams` garde `order.volume` tel quel si `volume` n'est pas fourni.
      if (volumeResult._tag === "Left") volumeUnchanged++;
      const volume = volumeResult._tag === "Right" ? volumeResult.right : undefined;

      await Effect.runPromise(
        client.amendOrder(toAmendOrderParams(order, { stopLoss, takeProfit, volume })),
      );
      amended++;
    }
    if (amended > 0) {
      const suffix =
        volumeUnchanged > 0
          ? ` (${volumeUnchanged} volume inchangé — sous le minimum pour ce risque%)`
          : "";
      setFeedback({
        kind: "success",
        message: `refresh ATR auto : ${amended} ordre(s) mis à jour${suffix}`,
      });
      void refreshMarket();
    }
  } catch (error) {
    setFeedback({ kind: "error", message: `échec refresh ATR auto : ${toMessage(error)}` });
  }
}

/**
 * Boucle alignée sur les clôtures M5 réelles (cf. nextAtrBoundaryMs) qui recalcule et pousse le
 * SL/TP des ordres ATR encore en attente (suivis par `atrTradeStore.ts`, alimenté par
 * `useTradeConfirm.ts`) — sans passer par une modale de confirmation, contrairement à l'ancien
 * `atrrefresh.ts` manuel : c'est un refresh en tâche de fond, pas une action tapée.
 */
export function useAtrAutoRefresh(opts: {
  positions: GetPositionsResult | undefined;
  enabled: boolean;
  refreshMarket: () => Promise<void>;
  atrPeriod: number;
  atrTimeframe: TrendbarPeriod;
}): AtrAutoRefresh {
  const { client, symbolId, instrument } = useCtrader();
  const { setFeedback } = useFeedback();
  const [trackedOrderIds, setTrackedOrderIds] = useState(EMPTY_TRACKED_IDS);

  // Relecture indépendante de la boucle de refresh ci-dessous — la colonne ATR de
  // OrdersTable.tsx doit refléter un trade tout juste confirmé sans attendre la prochaine
  // clôture de bougie.
  useInterval(() => {
    void fsRuntime.runPromise(readAtrTrades()).then((trades) => {
      setTrackedOrderIds(new Set(trades.map((t) => t.orderId)));
    });
  }, TRACKED_IDS_POLL_MS);

  // Toujours les dernières valeurs au moment où le timeout se déclenche, sans redémarrer la
  // chaîne à chaque changement de `positions` (qui bouge toutes les 3s, cf. useMarketData.ts) —
  // même raisonnement que le `callbackRef` de useInterval.ts, mais appliqué ici à un setTimeout
  // auto-réarmé plutôt qu'à `Schedule.spaced` : `Schedule.spaced` répète à intervalle fixe depuis
  // son démarrage, il ne peut pas se recaler sur une borne d'horloge murale à chaque tick.
  // `atrTimeframe` n'est PAS dans ce ref : un changement de timeframe doit redémarrer la boucle
  // ci-dessous avec la nouvelle cadence, contrairement à `atrPeriod` (n'affecte que le calcul, pas
  // le rythme de refresh) qui peut rester lu via le ref sans redémarrage.
  const lotSize = instrument?.lotSize ?? 100;
  const digits = instrument?.digits ?? 2;
  const latestRef = useRef({
    client,
    symbolId,
    positions: opts.positions,
    refreshMarket: opts.refreshMarket,
    setFeedback,
    atrPeriod: opts.atrPeriod,
    lotSize,
    digits,
  });
  latestRef.current = {
    client,
    symbolId,
    positions: opts.positions,
    refreshMarket: opts.refreshMarket,
    setFeedback,
    atrPeriod: opts.atrPeriod,
    lotSize,
    digits,
  };

  useEffect(() => {
    if (!opts.enabled) return;
    const intervalMs = atrRefreshMs(opts.atrTimeframe);
    let timeoutId: ReturnType<typeof setTimeout>;
    let cancelled = false;

    function scheduleNext() {
      const delay = Math.max(0, nextAtrBoundaryMs(Date.now(), intervalMs) - Date.now());
      timeoutId = setTimeout(() => {
        if (cancelled) return;
        const current = latestRef.current;
        if (current.symbolId && current.positions !== undefined) {
          void runRefresh(
            current.client,
            current.symbolId,
            current.positions,
            current.setFeedback,
            current.refreshMarket,
            current.atrPeriod,
            opts.atrTimeframe,
            current.lotSize,
            current.digits,
          );
        }
        scheduleNext();
      }, delay);
    }
    scheduleNext();

    return () => {
      cancelled = true;
      clearTimeout(timeoutId);
    };
  }, [opts.enabled, opts.atrTimeframe]);

  // Passe immédiate dès que `symbolId`/`positions` sont connus (connexion + premier fetch marché
  // sont async) : un ordre ATR déjà en attente d'une session précédente est ainsi rafraîchi tout
  // de suite plutôt que d'attendre potentiellement près de l'intervalle plein la prochaine clôture.
  const hasRunInitialRef = useRef(false);
  useEffect(() => {
    if (hasRunInitialRef.current) return;
    if (!opts.enabled || !symbolId || opts.positions === undefined) return;
    hasRunInitialRef.current = true;
    void runRefresh(
      client,
      symbolId,
      opts.positions,
      setFeedback,
      opts.refreshMarket,
      opts.atrPeriod,
      opts.atrTimeframe,
      lotSize,
      digits,
    );
  }, [
    opts.enabled,
    symbolId,
    opts.positions,
    client,
    opts.refreshMarket,
    opts.atrPeriod,
    opts.atrTimeframe,
    setFeedback,
    lotSize,
    digits,
  ]);

  return { enabled: opts.enabled, trackedOrderIds };
}
