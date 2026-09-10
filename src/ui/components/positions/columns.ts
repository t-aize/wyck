import type { CtraderPosition } from "../../../ctrader/schemas.ts";
import { toPips } from "../../../utils/priceMath.ts";

export const DEFAULT_PIP_SIZE = 0.1;

export const COLUMNS = {
  symbol: 12,
  id: 8,
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
  pipSize = DEFAULT_PIP_SIZE,
): string {
  if (mid === undefined) return "—";
  const candidates = [
    stopLoss === undefined ? undefined : { label: "SL", pips: toPips(mid - stopLoss, pipSize) },
    takeProfit === undefined ? undefined : { label: "TP", pips: toPips(mid - takeProfit, pipSize) },
  ].filter((c): c is { label: string; pips: number } => c !== undefined);
  if (candidates.length === 0) return "—";
  const nearest = candidates.reduce((a, b) => (a.pips <= b.pips ? a : b));
  return `${nearest.label} ${nearest.pips}p`;
}

/** Une position dont les champs à haute confiance ne résolvent pas du tout. */
export function isUnmapped(position: CtraderPosition): boolean {
  return (
    position.id === undefined ||
    position.side === undefined ||
    position.volume === undefined ||
    position.entry === undefined
  );
}
