import { describe, expect, test } from "bun:test";
import { activeKillzone, activeMarketSessions } from "./active.ts";

// Instants verified against real Intl.DateTimeFormat output (not hand-computed), 2026-01-15
// (Northern winter -> London=GMT/NY=EST no DST ambiguity, Sydney=AEDT/Tokyo=JST year-round).
function utc(hour: number): Date {
  return new Date(`2026-01-15T${String(hour).padStart(2, "0")}:00:00Z`);
}

function ids(sessions: { id: string }[]): string[] {
  return sessions.map((s) => s.id).sort();
}

describe("activeMarketSessions", () => {
  test("Sydney/Tokyo overlap (UTC 03:00 -> Sydney 14:00 AEDT, Tokyo 12:00 JST)", () => {
    expect(ids(activeMarketSessions(utc(3)))).toEqual(["sydney", "tokyo"]);
  });

  test("London only (UTC 09:00 -> London 09:00 GMT, NY 04:00 EST)", () => {
    expect(ids(activeMarketSessions(utc(9)))).toEqual(["london"]);
  });

  test("London/New York overlap (UTC 14:00 -> London 14:00, NY 09:00)", () => {
    expect(ids(activeMarketSessions(utc(14)))).toEqual(["london", "newYork"]);
  });

  test("New York only (UTC 18:00 -> NY 13:00, London 18:00 already closed)", () => {
    expect(ids(activeMarketSessions(utc(18)))).toEqual(["newYork"]);
  });
});

describe("activeKillzone", () => {
  test("asia killzone (NY 20:00)", () => {
    expect(activeKillzone(utc(1))?.id).toBe("asia");
  });

  test("no killzone active (NY 01:00, between asia and london windows)", () => {
    expect(activeKillzone(utc(6))).toBeUndefined();
  });

  test("london killzone (NY 03:00)", () => {
    expect(activeKillzone(utc(8))?.id).toBe("london");
  });

  test("newYork killzone (NY 08:00)", () => {
    expect(activeKillzone(utc(13))?.id).toBe("newYork");
  });

  test("londonClose killzone (NY 10:00)", () => {
    expect(activeKillzone(utc(15))?.id).toBe("londonClose");
  });
});
