import { Effect } from "effect";
import { useEffect, useMemo, useState } from "react";
import type {
  CtraderClientLive,
  CtraderTrendbar,
  GetTrendbarsParams,
} from "../../ctrader/client.ts";
import { dropFormingBar } from "../../domain/smc/bars.ts";
import { TREND_TIMEFRAMES, type TrendTimeframe } from "../../domain/smc/timeframes.ts";
import { computeTrendState, type TrendState } from "../../domain/smc/trend.ts";
import { toMessage } from "../../errors.ts";
import { useInterval } from "./useInterval.ts";

/** La tendance M5/M15/H1 évolue lentement (bougie la plus fine = 5 min) — inutile de poller au
 * rythme du prix. */
const TREND_POLL_MS = 60_000;

/** Même contrainte serveur que useMarketData/prepareTrade : get_trendbars refuse toute plage >
 * 720h — 700h laisse une marge de sécurité pour des fenêtres chaînées en parallèle. */
const REQUEST_CAP_MS = 700 * 60 * 60_000;

async function fetchHistory(
  client: CtraderClientLive,
  symbolId: number,
  period: GetTrendbarsParams["period"],
  historyMs: number,
  now: number,
): Promise<CtraderTrendbar[]> {
  const windowCount = Math.ceil(historyMs / REQUEST_CAP_MS);
  const windows = Array.from({ length: windowCount }, (_, i) => ({
    from: now - (i + 1) * REQUEST_CAP_MS,
    to: now - i * REQUEST_CAP_MS,
  })).reverse();

  const chunks = await Promise.all(
    windows.map(({ from, to }) =>
      Effect.runPromise(
        client.getTrendbars({
          symbolId,
          period,
          fromTimestamp: new Date(from).toISOString(),
          toTimestamp: new Date(to).toISOString(),
        }),
      ).then((result) => result.trendbars),
    ),
  );
  return chunks.flat();
}

function computeRow(closed: CtraderTrendbar[], tf: TrendTimeframe): TrendRow {
  const shared = { minSwingPct: tf.minSwingPct, displacementMult: tf.displacementMult };
  return {
    label: tf.label,
    // "swing" pilote le biais principal affiché (STRUCT/CONFIRME) ; "internal" est plus réactive
    // (fractale plus courte), sert de repère de timing d'entrée à part.
    swing: computeTrendState(closed, { left: tf.swingLength, right: tf.swingLength, ...shared }),
    internal: computeTrendState(closed, {
      left: tf.internalLength,
      right: tf.internalLength,
      ...shared,
    }),
  };
}

export interface TrendRow {
  label: string;
  swing: TrendState;
  internal: TrendState;
}

export interface Trend {
  rows: TrendRow[] | undefined;
  trendError: string | undefined;
  refreshTrend: () => Promise<void>;
}

export function useTrend(client: CtraderClientLive, symbolId: number | undefined): Trend {
  const [rows, setRows] = useState<TrendRow[]>();
  const [trendError, setTrendError] = useState<string>();

  const refreshTrend = useMemo(
    () => async () => {
      if (!symbolId) return;
      try {
        const now = Date.now();
        const results = await Promise.all(
          TREND_TIMEFRAMES.map(async (tf): Promise<TrendRow> => {
            const raw = await fetchHistory(client, symbolId, tf.period, tf.historyMs, now);
            const closed = dropFormingBar(raw, tf.periodMs, now);
            return computeRow(closed, tf);
          }),
        );
        setRows(results);
        setTrendError(undefined);
      } catch (error) {
        setTrendError(toMessage(error));
      }
    },
    [client, symbolId],
  );

  useInterval(() => void refreshTrend(), TREND_POLL_MS);

  // Même raison que useMarketData : le premier tick de useInterval tombe avant que symbolId soit
  // connu (connect() est async), sans quoi la tendance resterait vide jusqu'à TREND_POLL_MS après
  // la connexion.
  useEffect(() => {
    if (symbolId) void refreshTrend();
  }, [symbolId, refreshTrend]);

  return { rows, trendError, refreshTrend };
}
