import { TradeSide } from "../protocol/TradeSide.ts";

/**
 * Position ouverte, telle que l'app la manipule.
 *
 * Ce n'est **pas** le JSON brut du serveur. Le fil envoie
 * `{ positionId, entryPrice, tradeSide, … }` ; {@link mapPosition} projette
 * vers `id` / `entry` / `side`. Tous les champs peuvent être `undefined` : un
 * champ absent ou du mauvais type ne doit pas faire planter le panneau
 * (`isUnmapped` côté UI détecte le cas vide).
 *
 * Forme réelle confirmée (compte démo, XAUUSD) :
 * `{ positionId, symbolId, tradeSide, volume, entryPrice, stopLoss?, takeProfit?,
 * commission, swap }`. `volume` / `entryPrice` valent `0` sur le stub associé à
 * un ordre pending pas encore rempli.
 *
 * `volume` reste en **unités API** (1/100 d'unité d'actif de base). Le passage
 * en lots dépend du `lotSize` du symbole — côté app, après `get_symbols`.
 */
export interface CtraderPosition {
  id?: number;
  symbolId?: number;
  side?: TradeSide;
  volume?: number;
  entry?: number;
  stopLoss?: number;
  takeProfit?: number;
  swap?: number;
}

function readNumber(record: Record<string, unknown>, keys: string[]): number | undefined {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "number") return value;
  }
  return undefined;
}

function readTradeSide(record: Record<string, unknown>): TradeSide | undefined {
  const value = record.tradeSide;
  return value === TradeSide.BUY || value === TradeSide.SELL ? value : undefined;
}

/**
 * JSON brut d'une position → {@link CtraderPosition}.
 *
 * Pas de validation stricte : un objet pourri donne des `undefined`, jamais
 * d'exception. C'est volontaire — un parse qui échoue viderait tout le panneau.
 *
 * @example
 * mapPosition({ positionId: 7, tradeSide: "BUY", volume: 5000, entryPrice: 2000 })
 * // { id: 7, side: TradeSide.BUY, volume: 5000, entry: 2000, … }
 */
export function mapPosition(raw: unknown): CtraderPosition {
  const record = raw !== null && typeof raw === "object" ? (raw as Record<string, unknown>) : {};
  return {
    id: readNumber(record, ["positionId"]),
    symbolId: readNumber(record, ["symbolId"]),
    side: readTradeSide(record),
    volume: readNumber(record, ["volume"]),
    entry: readNumber(record, ["entryPrice"]),
    stopLoss: readNumber(record, ["stopLoss"]),
    takeProfit: readNumber(record, ["takeProfit"]),
    swap: readNumber(record, ["swap"]),
  };
}
