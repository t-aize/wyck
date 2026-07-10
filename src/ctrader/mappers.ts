import { toLots } from "../constants.ts";
import type { CtraderPosition } from "./schemas.ts";

/**
 * `CtraderPosition` reste typé `Record<string, unknown>` : sa forme exacte n'a
 * jamais pu être vérifiée contre un payload réel (pas de compte démo, aucune
 * position ouverte lors des tests). Ces lecteurs essaient plusieurs noms de
 * champ plausibles (convention "prix affiché" observée sur le reste de
 * l'API, cf. `CtraderOrder`/`CtraderDeal` dans ctrader/schemas.ts) et
 * retombent sur "—" plutôt que d'inventer une valeur. À corriger avec les
 * vrais noms dès qu'une position réelle passe par ici.
 */
function readNumber(record: Record<string, unknown>, keys: string[]): number | undefined {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "number") return value;
  }
  return undefined;
}

function readString(record: Record<string, unknown>, keys: string[]): string | undefined {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "string") return value;
  }
  return undefined;
}

export interface ReadPosition {
  id: number | undefined;
  symbol: string;
  side: string | undefined;
  volumeLots: number | undefined;
  entry: number | undefined;
  stopLoss: number | undefined;
  takeProfit: number | undefined;
  swap: number | undefined;
  pnl: number | undefined;
  openTimestamp: number | undefined;
}

export function readPosition(position: CtraderPosition): ReadPosition {
  return {
    id: readNumber(position, ["positionId", "id"]),
    symbol:
      readString(position, ["symbolName", "symbol"]) ??
      String(readNumber(position, ["symbolId"]) ?? "—"),
    side: readString(position, ["tradeSide", "side"]),
    // lotSize métaux = 100 → volume(1/100 unités) / 10000 = lots. Approximation valable pour XAUUSD,
    // seul symbole tradé par ce panel — à revoir si d'autres classes d'actifs sont ajoutées un jour.
    volumeLots: (() => {
      const raw = readNumber(position, ["volume"]);
      return raw === undefined ? undefined : toLots(raw);
    })(),
    entry: readNumber(position, ["entryPrice", "price"]),
    stopLoss: readNumber(position, ["stopLoss"]),
    takeProfit: readNumber(position, ["takeProfit"]),
    swap: readNumber(position, ["swap"]),
    pnl: readNumber(position, ["pnl", "profit", "grossProfit", "netProfit", "unrealizedNetProfit"]),
    openTimestamp: readNumber(position, ["openTimestamp", "utcLastUpdateTimestamp", "timestamp"]),
  };
}
