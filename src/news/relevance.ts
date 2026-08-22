import type { CalendarEvent, NewsImpact } from "./schemas.ts";
import { NewsImpactSchema } from "./schemas.ts";

export function classifyImpact(raw: string): NewsImpact {
  const parsed = NewsImpactSchema.safeParse(raw.trim().toLowerCase());
  return parsed.success ? parsed.data : "other";
}

/**
 * XAUUSD est coté en USD : les publications US sont structurellement les plus
 * corrélées. On marque aussi tout titre qui référence l'or explicitement
 * (rare dans ce calendrier, mais ça arrive : rapports miniers, réserves…).
 */
export const GOLD_KEYWORD = /gold|precious metal|\bxau\b/i;

export function isGoldRelevant(event: Pick<CalendarEvent, "country" | "title">): boolean {
  return event.country === "USD" || GOLD_KEYWORD.test(event.title);
}
