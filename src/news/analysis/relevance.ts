/**
 * Filtrage : cet event concerne-t-il **ce** symbole ?
 *
 * Deux portes, volontairement simples :
 * 1. la devise ForexFactory (`event.country`) est dans `profile.countries` ;
 * 2. **ou** le titre matche `profile.keywords` (or, bitcoin, pétrole, indice nommé).
 *
 * Le filtre d'affichage par défaut ajoute l'impact `high` : ce qui bouge
 * vraiment un marché, c'est NFP / CPI / FOMC / PIB, déjà tagués High côté FF.
 */

import type { CalendarEvent, NewsImpact } from "../calendar/schemas.ts";
import { NewsImpactSchema } from "../calendar/schemas.ts";
import type { NewsProfile } from "../profile/types.ts";

/**
 * Normalise l'impact brut du flux (`"High"`, `"  MEDIUM  "`) vers
 * `"high" | "medium" | "low"`, ou `"other"` si inconnu / vide.
 */
export function classifyImpact(raw: string): NewsImpact {
  const parsed = NewsImpactSchema.safeParse(raw.trim().toLowerCase());
  return parsed.success ? parsed.data : "other";
}

/**
 * Pertinent pour le profil : devise écoutée, ou mot-clé de classe dans le titre.
 *
 * @example
 * isRelevant({ country: "USD", title: "NFP" }, goldProfile)             // true
 * isRelevant({ country: "CHN", title: "Gold Reserves" }, goldProfile)   // true
 * isRelevant({ country: "EUR", title: "CPI m/m" }, goldProfile)         // false
 */
export function isRelevant(
  event: Pick<CalendarEvent, "country" | "title">,
  profile: NewsProfile,
): boolean {
  return profile.countries.includes(event.country) || profile.keywords.test(event.title);
}

/** `true` si l'impact normalisé est `high`. */
export function isHighImpact(event: Pick<CalendarEvent, "impact">): boolean {
  return classifyImpact(event.impact) === "high";
}

/**
 * Filtre par défaut du panneau : pertinent **et** fort impact.
 * Les deux étant vrais pour toute ligne affichée, l'UI n'a plus à répéter
 * « HIGH » sur chaque ligne.
 */
export function isDefaultVisible(event: CalendarEvent, profile: NewsProfile): boolean {
  return isRelevant(event, profile) && isHighImpact(event);
}
