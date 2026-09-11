/**
 * Timeframe d'une bougie, identifiant `get_trendbars.period`.
 *
 * Vit ici (pas dans l'app) : {@link GetTrendbarsParams} et le client MCP en
 * dépendent. L'UI (ATR, `settings atrtimeframe`) réimporte depuis `src/ctrader`.
 */
export enum TrendbarPeriod {
  M_1 = "M_1",
  M_5 = "M_5",
  M_15 = "M_15",
  M_30 = "M_30",
  H_1 = "H_1",
  H_4 = "H_4",
  D_1 = "D_1",
  W_1 = "W_1",
  MN_1 = "MN_1",
}

/**
 * Liste stable des {@link TrendbarPeriod}, dans l'ordre croissant.
 * Utile pour un `includes` / un `join` (commande `settings`) sans
 * `Object.values` (qui, sur un enum string, est déjà la liste des literals).
 */
export const TRENDBAR_PERIODS = [
  TrendbarPeriod.M_1,
  TrendbarPeriod.M_5,
  TrendbarPeriod.M_15,
  TrendbarPeriod.M_30,
  TrendbarPeriod.H_1,
  TrendbarPeriod.H_4,
  TrendbarPeriod.D_1,
  TrendbarPeriod.W_1,
  TrendbarPeriod.MN_1,
] as const;

/**
 * Durée d'une bougie, pour aligner un refresh (ATR) sur sa clôture.
 *
 * Approximatif pour `D_1` / `W_1` / `MN_1` : `Math.ceil(now / ms) * ms` suppose
 * des bougies de taille fixe alignées sur epoch 0, pas les horaires de séance.
 */
export const TRENDBAR_PERIOD_MS: Record<TrendbarPeriod, number> = {
  [TrendbarPeriod.M_1]: 60_000,
  [TrendbarPeriod.M_5]: 5 * 60_000,
  [TrendbarPeriod.M_15]: 15 * 60_000,
  [TrendbarPeriod.M_30]: 30 * 60_000,
  [TrendbarPeriod.H_1]: 60 * 60_000,
  [TrendbarPeriod.H_4]: 4 * 60 * 60_000,
  [TrendbarPeriod.D_1]: 24 * 60 * 60_000,
  [TrendbarPeriod.W_1]: 7 * 24 * 60 * 60_000,
  [TrendbarPeriod.MN_1]: 30 * 24 * 60 * 60_000,
};
