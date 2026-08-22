import { z } from "zod";

export const CalendarEventSchema = z.object({
  title: z.string(),
  country: z.string(),
  date: z.string(),
  impact: z.string(),
  forecast: z.string(),
  previous: z.string(),
});

/** Schéma décoré du `timestamp` dérivé (epoch ms, cf. `calendar.ts#fetchCalendar`) — sert à la fois
 * à valider le cache disque (qui stocke les events déjà décorés) et à dériver `CalendarEvent`, pour
 * ne pas retaper les 6 champs de `CalendarEventSchema` une deuxième fois dans une interface à part. */
export const DecoratedCalendarEventSchema = CalendarEventSchema.extend({ timestamp: z.number() });
export type CalendarEvent = z.infer<typeof DecoratedCalendarEventSchema>;

export const NewsImpactSchema = z.enum(["high", "medium", "low"]);
export type NewsImpact = z.infer<typeof NewsImpactSchema> | "other";

export const CacheFileSchema = z.object({
  fetchedAt: z.string(),
  events: z.array(DecoratedCalendarEventSchema),
});
export type CacheFile = z.infer<typeof CacheFileSchema>;
