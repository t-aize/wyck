import type { TrendbarPeriod } from "../protocol/TrendbarPeriod.ts";
import type { CtraderTrendbar } from "./CtraderTrendbar.ts";

/** Enveloppe de `get_trendbars`. */
export interface GetTrendbarsResult {
  trendbars: CtraderTrendbar[];
  symbolId: number;
  period: TrendbarPeriod;
}
