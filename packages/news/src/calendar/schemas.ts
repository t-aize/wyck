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
 * Champ texte ForexFactory : string, nombre, `null` / absent → `""`.
 * Un event dont un champ passe à `null` ne doit pas faire rater toute la semaine.
 */
const LooseString = z
  .union([z.string(), z.number()])
  .nullish()
  .transform((value) => (value == null ? "" : String(value)));

/**
 * Un event tel que renvoyé par `ff_calendar_thisweek.json`.
 *
 * `country` n'est pas un pays ISO-3166 : c'est le code **devise** ForexFactory
 * (`USD`, `EUR`, `GBP`…). `impact` arrive en casse libre (`High`, `medium`,
 * `Holiday`) — {@link classifyImpact} le normalise. `forecast` / `previous`
 * sont des chaînes libres (`"255K"`, `"0.3%"`) : le flux ne fournit jamais
 * `actual`.
 */
export const CalendarEventSchema = z.object({
  title: LooseString,
  country: LooseString,
  date: LooseString,
  impact: LooseString,
  forecast: LooseString,
  previous: LooseString,
});

/**
 * Event décoré du `timestamp` dérivé (epoch ms, cf. `fetchCalendar`).
 * Sert à la fois au cache disque (qui stocke déjà les events décorés) et au type
 * {@link CalendarEvent} consommé par l'UI — un seul schéma, pas une interface
 * recopiée à la main.
 */
export const DecoratedCalendarEventSchema = CalendarEventSchema.extend({
  timestamp: z.number().finite(),
});

/** Event de calendrier tel que l'app le manipule (champs ForexFactory + epoch ms). */
export type CalendarEvent = z.infer<typeof DecoratedCalendarEventSchema>;

/**
 * Niveaux d'impact ForexFactory. `"other"` n'est pas dans le flux : c'est le
 * repli de {@link classifyImpact} pour une valeur inconnue / vide (`Holiday`…).
 */
export const NewsImpactSchema = z.enum(["high", "medium", "low"]);

/** Impact normalisé, ou `"other"` si le flux a envoyé n'importe quoi. */
export type NewsImpact = z.infer<typeof NewsImpactSchema> | "other";

/** Fichier `calendar-cache.json` : horodatage du fetch + events déjà décorés. */
export const CacheFileSchema = z.object({
  fetchedAt: z.string(),
  events: z.array(DecoratedCalendarEventSchema),
});

/** Contenu du cache disque (TTL porté par `fetchedAt`, pas par le jour Paris). */
export type CacheFile = z.infer<typeof CacheFileSchema>;

/**
 * Valide les events un par un : un objet pourri / une date invalide est
 * ignoré, le reste de la semaine est conservé. `undefined` si `raw` n'est
 * pas un tableau (payload inattendu).
 */
export function decorateEvents(raw: unknown): CalendarEvent[] | undefined {
  if (!Array.isArray(raw)) return undefined;
  const events: CalendarEvent[] = [];
  for (const item of raw) {
    const parsed = CalendarEventSchema.safeParse(item);
    if (!parsed.success) continue;
    const timestamp = Date.parse(parsed.data.date);
    if (!Number.isFinite(timestamp)) continue;
    events.push({ ...parsed.data, timestamp });
  }
  events.sort((a, b) => a.timestamp - b.timestamp);
  return events;
}

/** Lecture permissive du JSON cache : `fetchedAt` + events décorés un par un. */
export function parseCacheFile(raw: unknown): CacheFile | undefined {
  if (raw === null || typeof raw !== "object") return undefined;
  const record = raw as { fetchedAt?: unknown; events?: unknown };
  if (typeof record.fetchedAt !== "string" || !Number.isFinite(Date.parse(record.fetchedAt))) {
    return undefined;
  }
  const events = decorateEvents(record.events);
  if (events === undefined) return undefined;
  return { fetchedAt: record.fetchedAt, events };
}
