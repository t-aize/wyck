import type { CtraderPosition } from "../../../ctrader/schemas.ts";
import { toPips } from "../../../utils/priceMath.ts";

export const COLUMNS = {
  symbol: 10,
  side: 7,
  volume: 9,
  entry: 10,
  sl: 10,
  tp: 10,
  dist: 14,
  /** Colonne "ATR" de OrdersTable.tsx uniquement (ordres en attente) — glyphe seul, pas de texte. */
  atr: 5,
} as const;

/** Distance (en pips) au SL/TP le plus proche du prix courant, avec l'étiquette du côté concerné. */
export function nearestPipsLabel(
  mid: number | undefined,
  stopLoss: number | undefined,
  takeProfit: number | undefined,
): string {
  if (mid === undefined) return "—";
  const candidates = [
    stopLoss === undefined ? undefined : { label: "SL", pips: toPips(mid - stopLoss) },
    takeProfit === undefined ? undefined : { label: "TP", pips: toPips(mid - takeProfit) },
  ].filter((c): c is { label: string; pips: number } => c !== undefined);
  if (candidates.length === 0) return "—";
  const nearest = candidates.reduce((a, b) => (a.pips <= b.pips ? a : b));
  return `${nearest.label} ${nearest.pips}p`;
}

/** Une position dont les champs à haute confiance (id/side/volumeLots/entry) ne résolvent pas du
 * tout signale que l'hypothèse de mapping (cf. `CtraderPositionSchema` dans ctrader/schemas.ts)
 * est cassée en pratique. */
export function isUnmapped(position: CtraderPosition): boolean {
  return (
    position.id === undefined ||
    position.side === undefined ||
    position.volumeLots === undefined ||
    position.entry === undefined
  );
}
