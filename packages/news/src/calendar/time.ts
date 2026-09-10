/**
 * Le calendrier est toujours affiché en heure de Paris, indépendamment du
 * fuseau système. Utiliser le fuseau machine ici serait incohérent avec
 * l'affichage si l'app tourne ailleurs qu'à Paris. Le TTL du cache, lui,
 * est en temps wall-clock (cf. `isCacheFresh`), pas une clé de jour.
 */

/** Fuseau unique du domaine news (affichage des events). */
export const PARIS_TZ = "Europe/Paris";

/**
 * Formate une date en `YYYY-MM-DD` dans {@link PARIS_TZ}.
 * Locale `en-CA` = ISO-like stable, pas un choix d'UI (l'UI a ses propres
 * `Intl.DateTimeFormat` `fr-FR`).
 */
export const parisDayKeyFormat = new Intl.DateTimeFormat("en-CA", {
  timeZone: PARIS_TZ,
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
});
