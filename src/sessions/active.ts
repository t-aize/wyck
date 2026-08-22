import { KILLZONES, MARKET_SESSIONS } from "./catalog.ts";
import { localHour, newYorkHour } from "./time.ts";
import type { Killzone, MarketSession } from "./types.ts";

/** Sessions de marché ouvertes à cet instant — 0 (marché fermé, cf. weekend), 1, ou 2 en cas de
 * chevauchement (ex : Londres/New York, Sydney/Tokyo). */
export function activeMarketSessions(date: Date): MarketSession[] {
  return MARKET_SESSIONS.filter((session) => {
    const hour = localHour(date, session.timeZone);
    return hour >= session.startHour && hour < session.endHour;
  });
}

/** Killzone ICT en cours, `undefined` en dehors de toute fenêtre (la majorité de la journée). */
export function activeKillzone(date: Date): Killzone | undefined {
  const hour = newYorkHour(date);
  return KILLZONES.find((kz) => hour >= kz.startHour && hour < kz.endHour);
}
