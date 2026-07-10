/** Calendrier économique ForexFactory (semaine en cours), avec cache disque journalier. */

import { homedir } from "node:os";
import { join } from "node:path";
import { z } from "zod";

const CALENDAR_URL = "https://nfs.faireconomy.media/ff_calendar_thisweek.json";
const CACHE_PATH = join(homedir(), ".aurum", "calendar-cache.json");

// Le calendrier est toujours raisonné en heure de Paris, indépendamment du fuseau système —
// autant pour l'affichage (NewsPanel) que pour la limite "un jour" du cache ci-dessous. Utiliser
// le fuseau système ici serait incohérent avec l'affichage si l'app tourne ailleurs qu'à Paris.
export const PARIS_TZ = "Europe/Paris";
const parisDayKeyFormat = new Intl.DateTimeFormat("en-CA", {
  timeZone: PARIS_TZ,
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
});

export function parisDayKey(date: Date): string {
  return parisDayKeyFormat.format(date);
}

const CalendarEventSchema = z.object({
  title: z.string(),
  country: z.string(),
  date: z.string(),
  impact: z.string(),
  forecast: z.string(),
  previous: z.string(),
});

export interface CalendarEvent {
  title: string;
  country: string;
  date: string;
  impact: string;
  forecast: string;
  previous: string;
  /** epoch ms, dérivé de `date` */
  timestamp: number;
}

export type NewsImpact = "high" | "medium" | "low" | "other";

export function classifyImpact(raw: string): NewsImpact {
  const value = raw.trim().toLowerCase();
  if (value === "high" || value === "medium" || value === "low") return value;
  return "other";
}

/**
 * XAUUSD est coté en USD : les publications US sont structurellement les plus
 * corrélées. On marque aussi tout titre qui référence l'or explicitement
 * (rare dans ce calendrier, mais ça arrive : rapports miniers, réserves…).
 */
const GOLD_KEYWORD = /gold|precious metal|\bxau\b/i;

export function isGoldRelevant(event: Pick<CalendarEvent, "country" | "title">): boolean {
  return event.country === "USD" || GOLD_KEYWORD.test(event.title);
}

const CacheFileSchema = z.object({
  fetchedAt: z.string(),
  events: z.array(CalendarEventSchema.extend({ timestamp: z.number() })),
});

async function readCache(): Promise<{ fetchedAt: string; events: CalendarEvent[] } | undefined> {
  try {
    const file = Bun.file(CACHE_PATH);
    if (!(await file.exists())) return undefined;
    return CacheFileSchema.parse(await file.json());
  } catch {
    // Cache absent, corrompu ou d'un format antérieur : on retombe sur un fetch réseau.
    return undefined;
  }
}

async function writeCache(events: CalendarEvent[]): Promise<void> {
  const payload = { fetchedAt: new Date().toISOString(), events };
  await Bun.write(CACHE_PATH, JSON.stringify(payload, null, 2));
}

/**
 * Le calendrier ("cette semaine") ne change quasiment pas d'un jour à l'autre —
 * un fetch par jour calendaire suffit largement et évite le rate limit du
 * serveur. `force: true` (commande /refresh) bypasse le cache same-day, mais
 * retombe quand même sur les données en cache si le réseau échoue.
 */
export async function fetchCalendar(options: { force?: boolean } = {}): Promise<CalendarEvent[]> {
  const cached = await readCache();
  if (
    !options.force &&
    cached &&
    parisDayKey(new Date(cached.fetchedAt)) === parisDayKey(new Date())
  ) {
    return cached.events;
  }

  try {
    const response = await fetch(CALENDAR_URL);
    if (response.status === 429) {
      const retryAfter = response.headers.get("retry-after");
      const wait = retryAfter ? ` (réessai dans ${retryAfter}s)` : "";
      throw new Error(`Calendrier économique : limité par le serveur${wait}`);
    }
    if (!response.ok) {
      throw new Error(`Calendrier économique : HTTP ${response.status}`);
    }

    const raw = await response.json();
    const parsed = z.array(CalendarEventSchema).parse(raw);
    const events = parsed
      .map((event) => ({ ...event, timestamp: new Date(event.date).getTime() }))
      .sort((a, b) => a.timestamp - b.timestamp);

    await writeCache(events).catch(() => {});
    return events;
  } catch (error) {
    // Réseau en échec (rate limit, offline…) : mieux vaut de la donnée périmée qu'une erreur.
    if (cached) return cached.events;
    throw error;
  }
}
