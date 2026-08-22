// Le calendrier est toujours raisonné en heure de Paris, indépendamment du fuseau système —
// autant pour l'affichage (NewsPanel) que pour la limite "un jour" du cache (calendar.ts). Utiliser
// le fuseau système ici serait incohérent avec l'affichage si l'app tourne ailleurs qu'à Paris.
export const PARIS_TZ = "Europe/Paris";
export const parisDayKeyFormat = new Intl.DateTimeFormat("en-CA", {
  timeZone: PARIS_TZ,
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
});
