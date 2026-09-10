/**
 * Le calendrier est toujours raisonné en heure de Paris, indépendamment du
 * fuseau système — autant pour l'affichage (NewsPanel) que pour la limite
 * « un jour » du cache. Utiliser le fuseau machine ici serait incohérent
 * avec l'affichage si l'app tourne ailleurs qu'à Paris.
 */

/** Fuseau unique du domaine news (affichage + clé de cache same-day). */
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
