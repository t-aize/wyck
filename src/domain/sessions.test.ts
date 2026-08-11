import { describe, expect, test } from "bun:test";
import { activeKillzone, activeMarketSessions, localHour, newYorkHour } from "./sessions.ts";

describe("newYorkHour", () => {
  test("hiver (EST, UTC-5) : 12:00 UTC ⇒ 7h à New York", () => {
    expect(newYorkHour(new Date("2026-01-15T12:00:00Z"))).toBe(7);
  });

  test("été (EDT, UTC-4) : 12:00 UTC ⇒ 8h à New York", () => {
    expect(newYorkHour(new Date("2026-07-15T12:00:00Z"))).toBe(8);
  });

  test("minuit à New York ⇒ 0, pas 24", () => {
    // 2026-01-15T05:00:00Z = 2026-01-15T00:00:00 EST
    expect(newYorkHour(new Date("2026-01-15T05:00:00Z"))).toBe(0);
  });

  test("bascule EST→EDT (2026-03-08 2h locale) : même instant UTC change d'heure NY avant/après", () => {
    // 06:59 UTC le 8 mars 2026 est encore avant la bascule (1:59 EST) ; 07:00 UTC est après (3:00 EDT).
    expect(newYorkHour(new Date("2026-03-08T06:59:00Z"))).toBe(1);
    expect(newYorkHour(new Date("2026-03-08T07:00:00Z"))).toBe(3);
  });

  test("bascule EDT→EST (2026-11-01) : l'heure locale recule d'une heure", () => {
    // 05:59 UTC le 1er novembre 2026 est encore 1:59 EDT ; 06:00 UTC redevient 1:00 EST.
    expect(newYorkHour(new Date("2026-11-01T05:59:00Z"))).toBe(1);
    expect(newYorkHour(new Date("2026-11-01T06:00:00Z"))).toBe(1);
  });
});

describe("activeKillzone", () => {
  test("20h-0h NY ⇒ killzone asiatique", () => {
    expect(activeKillzone(new Date("2026-01-15T01:00:00Z"))?.id).toBe("asia");
  });

  test("2h-5h NY ⇒ killzone londonienne", () => {
    expect(activeKillzone(new Date("2026-01-15T08:00:00Z"))?.id).toBe("london");
  });

  test("7h-10h NY ⇒ killzone new-yorkaise", () => {
    expect(activeKillzone(new Date("2026-01-15T13:00:00Z"))?.id).toBe("newYork");
  });

  test("10h-12h NY ⇒ killzone de clôture londonienne", () => {
    expect(activeKillzone(new Date("2026-01-15T16:00:00Z"))?.id).toBe("londonClose");
  });

  test("hors fenêtre (ex: 14h NY) ⇒ undefined", () => {
    expect(activeKillzone(new Date("2026-01-15T19:00:00Z"))).toBeUndefined();
  });

  test("frontière basse incluse, frontière haute exclue", () => {
    expect(activeKillzone(new Date("2026-01-15T12:00:00Z"))?.id).toBe("newYork"); // 7h pile
    expect(activeKillzone(new Date("2026-01-15T15:00:00Z"))?.id).toBe("londonClose"); // 10h pile
  });

  test("même instant UTC, killzone différente en hiver vs été (DST pris en compte)", () => {
    // 14:00 UTC = 9h NY en hiver (newYork killzone) mais 10h NY en été (londonClose).
    expect(activeKillzone(new Date("2026-01-15T14:00:00Z"))?.id).toBe("newYork");
    expect(activeKillzone(new Date("2026-07-15T14:00:00Z"))?.id).toBe("londonClose");
  });
});

describe("localHour — DST hémisphère sud (Sydney, sens inverse de l'Europe/US)", () => {
  test("été austral (AEDT, UTC+11) : 21:00 UTC ⇒ 8h à Sydney", () => {
    expect(localHour(new Date("2026-01-15T21:00:00Z"), "Australia/Sydney")).toBe(8);
  });

  test("hiver austral (AEST, UTC+10), même instant UTC ⇒ 7h à Sydney", () => {
    expect(localHour(new Date("2026-07-15T21:00:00Z"), "Australia/Sydney")).toBe(7);
  });
});

describe("activeMarketSessions", () => {
  test("09:00 UTC hiver ⇒ Sydney et Tokyo ouverts, Londres et New York fermés", () => {
    const ids = activeMarketSessions(new Date("2026-01-15T01:00:00Z")).map((s) => s.id);
    expect(ids.sort()).toEqual(["sydney", "tokyo"]);
  });

  test("chevauchement Londres/New York, hiver (13h-17h UTC)", () => {
    const ids = activeMarketSessions(new Date("2026-01-15T14:00:00Z")).map((s) => s.id);
    expect(ids.sort()).toEqual(["london", "newYork"]);
  });

  test("chevauchement Londres/New York, été (l'heure UTC de la fenêtre glisse avec la DST)", () => {
    const ids = activeMarketSessions(new Date("2026-07-15T14:00:00Z")).map((s) => s.id);
    expect(ids.sort()).toEqual(["london", "newYork"]);
  });

  test("aucune session ouverte (creux entre la clôture NY et l'ouverture Sydney, été)", () => {
    // En hiver les 4 fenêtres se recouvrent assez pour ne jamais laisser de trou (vérifié
    // heure par heure) ; l'été les décale et fait apparaître ce creux d'1h.
    expect(activeMarketSessions(new Date("2026-07-15T21:00:00Z"))).toEqual([]);
  });

  test("Tokyo seul, hors chevauchement avec Sydney ou Londres", () => {
    const ids = activeMarketSessions(new Date("2026-01-15T06:00:00Z")).map((s) => s.id);
    expect(ids).toEqual(["tokyo"]);
  });
});
