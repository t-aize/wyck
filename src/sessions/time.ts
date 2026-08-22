/**
 * Heure locale dérivée via `Intl.DateTimeFormat` + fuseau IANA plutôt qu'un offset UTC codé en
 * dur : le fuseau gère automatiquement le basculement heure d'hiver/été de chaque place, et ces
 * basculements ne tombent ni aux mêmes dates ni dans le même sens d'une place à l'autre
 * (US/UK : mars→novembre ; Australie, hémisphère sud : octobre→avril, sens inverse ; Tokyo :
 * aucun DST) — un offset fixe dériverait plusieurs fois par an, différemment selon la place.
 */

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
