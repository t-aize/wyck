/**
 * Timeframes cTrader (`get_trendbars.period`).
 *
 * Vivent ici, pas dans l'app : le schéma du flux ({@link GetTrendbarsResultSchema})
 * et le client MCP en dépendent. L'UI (ATR, `settings atrtimeframe`) réimporte
 * depuis `@aurum/ctrader`.
 */

/** Identifiants de période acceptés par `get_trendbars`. */
export const TRENDBAR_PERIODS = [
  "M_1",
  "M_5",
  "M_15",
  "M_30",
  "H_1",
  "H_4",
  "D_1",
  "W_1",
  "MN_1",
] as const;

/** Une des valeurs de {@link TRENDBAR_PERIODS}. */
export type TrendbarPeriod = (typeof TRENDBAR_PERIODS)[number];

/**
 * Durée d'une bougie par timeframe — pour aligner un refresh (ATR) sur sa clôture.
 *
 * Approximatif pour `D_1` / `W_1` / `MN_1` : `Math.ceil(now / ms) * ms` suppose
 * des bougies de taille fixe alignées sur epoch 0, pas les horaires de séance.
 */
export const TRENDBAR_PERIOD_MS: Record<TrendbarPeriod, number> = {
  M_1: 60_000,
  M_5: 5 * 60_000,
  M_15: 15 * 60_000,
  M_30: 30 * 60_000,
  H_1: 60 * 60_000,
  H_4: 4 * 60 * 60_000,
  D_1: 24 * 60 * 60_000,
  W_1: 7 * 24 * 60 * 60_000,
  MN_1: 30 * 24 * 60 * 60_000,
};
