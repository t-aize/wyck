/**
 * Sessions de marché forex + killzones ICT. Toutes deux dérivées de l'heure locale de la place
 * concernée via `Intl.DateTimeFormat` + fuseau IANA plutôt qu'un offset UTC codé en dur : le
 * fuseau gère automatiquement le basculement heure d'hiver/été de chaque place, et ces
 * basculements ne tombent ni aux mêmes dates ni dans le même sens d'une place à l'autre
 * (US/UK : mars→novembre ; Australie, hémisphère sud : octobre→avril, sens inverse ; Tokyo :
 * aucun DST) — un offset fixe dériverait plusieurs fois par an, différemment selon la place.
 *
 * — Sessions (`MARKET_SESSIONS`) : horaires d'ouverture usuels des quatre places qui, mises
 *   bout à bout, couvrent le marché 24h/24 (Sydney, Tokyo, Londres, New York — cf. sources).
 *   `activeMarketSessions` peut renvoyer plusieurs sessions à la fois lors d'un chevauchement
 *   (ex : Londres/New York 13h-17h UTC hiver).
 * — Killzones ICT (`KILLZONES`) : sous-fenêtres plus étroites, à forte probabilité de mouvement
 *   selon la méthodologie ICT, toutes définies en heure de New York par convention. Au plus une
 *   active à la fois (fenêtres disjointes).
 *
 * Sources (horaires locaux usuels, confirmés par recherche) :
 * - Sydney   08:00-17:00 AEST/AEDT (Australia/Sydney)
 * - Tokyo    09:00-18:00 JST, pas de DST (Asia/Tokyo)
 * - Londres  08:00-17:00 GMT/BST (Europe/London)
 * - New York 08:00-17:00 EST/EDT (America/New_York)
 */

export type MarketSessionId = "sydney" | "tokyo" | "london" | "newYork";

export interface MarketSession {
  id: MarketSessionId;
  label: string;
  timeZone: string;
  /** Heure locale de la place (0-23), fenêtre [startHour, endHour[. */
  startHour: number;
  endHour: number;
}

export const MARKET_SESSIONS: MarketSession[] = [
  { id: "sydney", label: "SYD", timeZone: "Australia/Sydney", startHour: 8, endHour: 17 },
  { id: "tokyo", label: "TOK", timeZone: "Asia/Tokyo", startHour: 9, endHour: 18 },
  { id: "london", label: "LON", timeZone: "Europe/London", startHour: 8, endHour: 17 },
  { id: "newYork", label: "NY", timeZone: "America/New_York", startHour: 8, endHour: 17 },
];

export type KillzoneId = "asia" | "london" | "newYork" | "londonClose";

export interface Killzone {
  id: KillzoneId;
  label: string;
  /** Heure de New York (0-23), fenêtre [startHour, endHour[. */
  startHour: number;
  endHour: number;
}

export const KILLZONES: Killzone[] = [
  { id: "asia", label: "ASIA", startHour: 20, endHour: 24 },
  { id: "london", label: "LDN", startHour: 2, endHour: 5 },
  { id: "newYork", label: "NY", startHour: 7, endHour: 10 },
  { id: "londonClose", label: "LDN CLOSE", startHour: 10, endHour: 12 },
];

const NEW_YORK_TZ = "America/New_York";

// Un formateur par fuseau, construit à la demande et mis en cache : `Intl.DateTimeFormat` est
// coûteux à instancier mais réutilisable indéfiniment pour un fuseau donné.
const hourFormatCache = new Map<string, Intl.DateTimeFormat>();

function hourFormatFor(timeZone: string): Intl.DateTimeFormat {
  let format = hourFormatCache.get(timeZone);
  if (!format) {
    // hourCycle: "h23" plutôt que hour12: false — évite un quirk connu de certaines ICU qui
    // rendent minuit "24" au lieu de "0" en hour12: false, ce qui casserait la comparaison
    // d'intervalle.
    format = new Intl.DateTimeFormat("en-US", { timeZone, hourCycle: "h23", hour: "numeric" });
    hourFormatCache.set(timeZone, format);
  }
  return format;
}

/** Heure locale (0-23) dans le fuseau IANA donné, DST géré automatiquement. */
export function localHour(date: Date, timeZone: string): number {
  return Number(hourFormatFor(timeZone).format(date));
}

/** Heure locale à New York (0-23) — raccourci de `localHour` pour les killzones ICT. */
export function newYorkHour(date: Date): number {
  return localHour(date, NEW_YORK_TZ);
}

/** Sessions de marché ouvertes à cet instant — 0 (marché fermé, cf. weekend), 1, ou 2 en cas de
 * chevauchement (ex : Londres/New York, Sydney/Tokyo). */
export function activeMarketSessions(date: Date): MarketSession[] {
  return MARKET_SESSIONS.filter((session) => {
    const hour = localHour(date, session.timeZone);
    return hour >= session.startHour && hour < session.endHour;
  });
}

/** Killzone ICT en cours, `undefined` en dehors de toute fenêtre (la majorité de la journée). */
export function activeKillzone(date: Date): Killzone | undefined {
  const hour = newYorkHour(date);
  return KILLZONES.find((kz) => hour >= kz.startHour && hour < kz.endHour);
}
