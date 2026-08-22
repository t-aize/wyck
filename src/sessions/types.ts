export type PlaceId = "sydney" | "tokyo" | "london" | "newYork";

/**
 * Label partagé pour les 4 places, utilisé à la fois dans `MARKET_SESSIONS` et (pour Londres/
 * New York) dans `KILLZONES` — source unique pour que les deux ne dérivent jamais l'un de
 * l'autre (avant ce fichier : "LON" côté session, "LDN" côté killzone, pour la même place).
 */
export const PLACE_LABEL: Record<PlaceId, string> = {
  sydney: "SYD",
  tokyo: "TOK",
  london: "LON",
  newYork: "NY",
};

export type MarketSessionId = PlaceId;

export interface MarketSession {
  id: MarketSessionId;
  label: string;
  timeZone: string;
  /** Heure locale de la place (0-23), fenêtre [startHour, endHour[. */
  startHour: number;
  endHour: number;
}

export type KillzoneId = "asia" | "london" | "newYork" | "londonClose";

export interface Killzone {
  id: KillzoneId;
  label: string;
  /** Heure de New York (0-23), fenêtre [startHour, endHour[. */
  startHour: number;
  endHour: number;
}
