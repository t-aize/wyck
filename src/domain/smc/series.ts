/** Séries numériques bas niveau partagées par atr.ts et filters.ts (ADX) — extrait de trend.ts. */

import type { CtraderTrendbar } from "../../ctrader/schemas.ts";

export function trueRangeSeries(bars: CtraderTrendbar[]): number[] {
  const tr: number[] = [0];
  for (let i = 1; i < bars.length; i++) {
    tr.push(
      Math.max(
        bars[i]!.high - bars[i]!.low,
        Math.abs(bars[i]!.high - bars[i - 1]!.close),
        Math.abs(bars[i]!.low - bars[i - 1]!.close),
      ),
    );
  }
  return tr;
}

export function rollingMean(values: number[], period: number): (number | undefined)[] {
  const result: (number | undefined)[] = new Array(values.length).fill(undefined);
  let sum = 0;
  for (let i = 0; i < values.length; i++) {
    sum += values[i]!;
    if (i >= period) sum -= values[i - period]!;
    if (i >= period - 1) result[i] = sum / period;
  }
  return result;
}
