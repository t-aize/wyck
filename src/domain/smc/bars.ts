import type { CtraderTrendbar } from "../../ctrader/schemas.ts";

/** get_trendbars renvoie la bougie en formation en dernière position ; on l'écarte, sinon un pivot pourrait "apparaître" puis disparaître d'un poll à l'autre. */
export function dropFormingBar(
  bars: CtraderTrendbar[],
  periodMs: number,
  now: number,
): CtraderTrendbar[] {
  const last = bars[bars.length - 1];
  return last && last.timestamp + periodMs > now ? bars.slice(0, -1) : bars;
}
