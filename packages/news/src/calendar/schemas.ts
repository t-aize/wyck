/**
 * Formes JSON du calendrier : payload ForexFactory (entrant) et cache disque (local).
 *
 * Deux schémas d'event volontairement distincts :
 * - {@link CalendarEventSchema} — ce que le serveur envoie (6 champs texte, `date` ISO).
 * - {@link DecoratedCalendarEventSchema} — après `fetchCalendar` : le même, plus `timestamp`
 *   (epoch ms) pour trier/comparer sans reparser `date` à chaque rendu.
 */

import { z } from "zod";

/**
 * Un event tel que renvoyé par `ff_calendar_thisweek.json`.
 *
 * `country` n'est pas un pays ISO-3166 : c'est le code **devise** ForexFactory
 * (`USD`, `EUR`, `GBP`…). `impact` arrive en casse libre (`High`, `medium`) —
 * {@link classifyImpact} le normalise. `forecast` / `previous` sont des chaînes
 * libres (`"255K"`, `"0.3%"`) : le flux ne fournit jamais `actual`.
 */
export const CalendarEventSchema = z.object({
  title: z.string(),
  country: z.string(),
  date: z.string(),
  impact: z.string(),
  forecast: z.string(),
  previous: z.string(),
});

/**
 * Event décoré du `timestamp` dérivé (epoch ms, cf. `fetchCalendar`).
 * Sert à la fois au cache disque (qui stocke déjà les events décorés) et au type
 * {@link CalendarEvent} consommé par l'UI — un seul schéma, pas une interface
 * recopiée à la main.
 */
export const DecoratedCalendarEventSchema = CalendarEventSchema.extend({ timestamp: z.number() });

/** Event de calendrier tel que l'app le manipule (champs ForexFactory + epoch ms). */
export type CalendarEvent = z.infer<typeof DecoratedCalendarEventSchema>;

/**
 * Niveaux d'impact ForexFactory. `"other"` n'est pas dans le flux : c'est le
 * repli de {@link classifyImpact} pour une valeur inconnue / vide.
 */
export const NewsImpactSchema = z.enum(["high", "medium", "low"]);

/** Impact normalisé, ou `"other"` si le flux a envoyé n'importe quoi. */
export type NewsImpact = z.infer<typeof NewsImpactSchema> | "other";

/** Fichier `calendar-cache.json` : horodatage du fetch + events déjà décorés. */
export const CacheFileSchema = z.object({
  fetchedAt: z.string(),
  events: z.array(DecoratedCalendarEventSchema),
});

/** Contenu du cache disque journalier. */
export type CacheFile = z.infer<typeof CacheFileSchema>;
