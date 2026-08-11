import { Effect } from "effect";
import { useEffect, useMemo, useState } from "react";
import type {
  CtraderClientLive,
  CtraderTrendbar,
  GetTrendbarsParams,
} from "../../ctrader/client.ts";
import { dropFormingBar } from "../../domain/smc/bars.ts";
import type { AtrTimeframeLabel } from "../../domain/smc/timeframes.ts";
import { TREND_TIMEFRAMES, type TrendTimeframe } from "../../domain/smc/timeframes.ts";
import {
  computeAtr,
  computeTrendState,
  type Trend as TrendDirection,
  type TrendState,
} from "../../domain/smc/trend.ts";
import { toMessage } from "../../errors.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { useInterval } from "./useInterval.ts";

/** La tendance M5/M15/H1 évolue lentement (bougie la plus fine = 5 min) — inutile de poller au
 * rythme du prix. */
const TREND_POLL_MS = 60_000;

/** Plage max par appel côté serveur : get_trendbars refuse toute plage > 720h — 700h laisse une
 * marge de sécurité pour des fenêtres chaînées en parallèle. */
const TIME_RANGE_CAP_MS = 700 * 60 * 60_000;

/** Second plafond, indépendant du premier et documenté séparément (cf. doc MCP cTrader,
 * "Analysis") : chaque appel renvoie au plus ~1000 bougies, quelle que soit la plage demandée —
 * silencieusement (pas d'erreur, juste moins de bougies que prévu). Sans en tenir compte, 700h
 * revient complet sur H1 (≤700 bougies) mais tronqué à ~1/3 sur M15 (2800 attendues) et ~1/8 sur
 * M5 (8400 attendues) : la structure SMC calculée dessus a des trous silencieux — c'est la cause
 * du support/résistance figé sur un pivot ancien vu sur M5/M15. Marge sous 1000 pour rester
 * prudent (le seuil réel n'est pas garanti par la doc au chiffre près).
 */
const MAX_BARS_PER_REQUEST = 900;

/** Fenêtre effective par appel : la plus contraignante des deux plafonds, en bougies de `periodMs`
 * (donc plus courte en temps sur les TF fines — M5/M15 chaînent davantage de fenêtres que H1 pour
 * la même profondeur d'historique, plutôt que de recevoir des fenêtres tronquées). */
export function requestCapMs(periodMs: number): number {
  return Math.min(TIME_RANGE_CAP_MS, MAX_BARS_PER_REQUEST * periodMs);
}

async function fetchHistory(
  client: CtraderClientLive,
  symbolId: number,
  period: GetTrendbarsParams["period"],
  periodMs: number,
  historyMs: number,
  now: number,
): Promise<CtraderTrendbar[]> {
  const capMs = requestCapMs(periodMs);
  const windowCount = Math.ceil(historyMs / capMs);
  const windows = Array.from({ length: windowCount }, (_, i) => ({
    from: now - (i + 1) * capMs,
    to: now - i * capMs,
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

/** Biais H1 confirmé (méthode événementielle + règle CHoCH→BOS) — "commande" la hiérarchie
 * multi-timeframe, cf. domain/trading.ts#conflictsWithHtfBias pour la règle elle-même. Partagé par
 * useOrderActions.ts (avertissement à la confirmation d'un trade) et TrendPanel.tsx (annotation
 * "contre le biais H1" par TF) — même source, pas une règle dupliquée deux fois. */
export function h1ConfirmedBias(rows: TrendRow[] | undefined): TrendDirection {
  return rows?.find((row) => row.label === "H1")?.swing.confirmedEvent ?? 0;
}

export interface Trend {
  rows: TrendRow[] | undefined;
  /** ATR(atrPeriod) le plus récent sur `atrTimeframe`, échelle brute x10^5 (cf.
   * domain/smc/trend.ts#computeAtr) — consommé par le mode ATR (domain/trading.ts#prepareAtrTrade,
   * useAtrOrderTracking.ts). */
  atr: number | undefined;
  trendError: string | undefined;
  refreshTrend: () => Promise<void>;
}

/**
 * `atrPeriod`/`atrTimeframe` pilotent l'ATR exposé (réglages `atr period`/`atr timeframe`, cf.
 * commands.ts) — défauts 14/M5 (`DEFAULT_ATR_SETTINGS` dans config.ts), pas dupliqués ici.
 * `atrTimeframe` ne peut être qu'un des TF déjà suivis par TREND_TIMEFRAMES (M5/M15/H1) : leurs
 * bougies sont de toute façon déjà fetchées ici, pas besoin d'un appel réseau dédié.
 */
export function useTrend(atrPeriod = 14, atrTimeframe: AtrTimeframeLabel = "M5"): Trend {
  const { client, symbolId } = useCtrader();
  const [rows, setRows] = useState<TrendRow[]>();
  const [atr, setAtr] = useState<number>();
  const [trendError, setTrendError] = useState<string>();

  const refreshTrend = useMemo(
    () => async () => {
      if (!symbolId) return;
      try {
        const now = Date.now();
        const results = await Promise.all(
          TREND_TIMEFRAMES.map(async (tf) => {
            const raw = await fetchHistory(
              client,
              symbolId,
              tf.period,
              tf.periodMs,
              tf.historyMs,
              now,
            );
            const closed = dropFormingBar(raw, tf.periodMs, now);
            return { tf, closed, row: computeRow(closed, tf) };
          }),
        );
        setRows(results.map((r) => r.row));
        const atrSource = results.find((r) => r.tf.label === atrTimeframe);
        setAtr(atrSource ? computeAtr(atrSource.closed, atrPeriod) : undefined);
        setTrendError(undefined);
      } catch (error) {
        setTrendError(toMessage(error));
      }
    },
    [client, symbolId, atrPeriod, atrTimeframe],
  );

  useInterval(() => void refreshTrend(), TREND_POLL_MS);

  // Même raison que useMarketData : le premier tick de useInterval tombe avant que symbolId soit
  // connu (connect() est async), sans quoi la tendance resterait vide jusqu'à TREND_POLL_MS après
  // la connexion.
  useEffect(() => {
    if (symbolId) void refreshTrend();
  }, [symbolId, refreshTrend]);

  return { rows, atr, trendError, refreshTrend };
}
