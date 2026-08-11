/** Filtres classiques (pas du SMC) — EMA stack + ADX. Extrait de trend.ts. */

import type { CtraderTrendbar } from "../../ctrader/client.ts";
import { rollingMean, trueRangeSeries } from "./series.ts";

/** ADX > ce seuil ~ marché "en tendance" (seuil usuel, pas une loi physique). */
export const ADX_TRENDING_THRESHOLD = 25;

function computeEma(values: number[], span: number): number {
  const alpha = 2 / (span + 1);
  let ema = values[0]!;
  for (let i = 1; i < values.length; i++) ema = values[i]! * alpha + ema * (1 - alpha);
  return ema;
}

/** EMA rapide > EMA lente sur la clôture — direction "classique", pas du SMC. `undefined` si pas
 * assez de bougies pour la plus lente des deux. */
export function computeEmaStack(
  bars: CtraderTrendbar[],
  { fast = 20, slow = 50 }: { fast?: number; slow?: number } = {},
): boolean | undefined {
  if (bars.length < slow) return undefined;
  const closes = bars.map((b) => b.close);
  return computeEma(closes, fast) > computeEma(closes, slow);
}

/**
 * ADX(period) façon Wilder simplifié — moyenne mobile simple plutôt que le lissage RMA classique,
 * comme la référence. Sert de garde-fou pour éviter de lire un signal structurel valide dans un
 * marché qui, statistiquement, ne trend pas (cf. ADX_TRENDING_THRESHOLD). `undefined` si pas assez
 * de bougies.
 */
export function computeAdx(bars: CtraderTrendbar[], period = 14): number | undefined {
  if (bars.length < period * 2 + 1) return undefined;

  const plusDm: number[] = [0];
  const minusDm: number[] = [0];
  for (let i = 1; i < bars.length; i++) {
    const upMove = bars[i]!.high - bars[i - 1]!.high;
    const downMove = bars[i - 1]!.low - bars[i]!.low;
    let plus = Math.max(upMove, 0);
    let minus = Math.max(downMove, 0);
    if (plus - minus <= 0) plus = 0;
    if (minus - plus <= 0) minus = 0;
    plusDm.push(plus);
    minusDm.push(minus);
  }

  const atr = rollingMean(trueRangeSeries(bars), period);
  const plusDmAvg = rollingMean(plusDm, period);
  const minusDmAvg = rollingMean(minusDm, period);

  const dx = bars.map((_, i) => {
    const a = atr[i];
    const pd = plusDmAvg[i];
    const md = minusDmAvg[i];
    if (a === undefined || pd === undefined || md === undefined || a === 0) return 0;
    const plusDi = 100 * (pd / a);
    const minusDi = 100 * (md / a);
    const sum = plusDi + minusDi;
    return sum === 0 ? 0 : (100 * Math.abs(plusDi - minusDi)) / sum;
  });

  return rollingMean(dx, period).at(-1);
}
