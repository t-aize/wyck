import { describe, expect, test } from "bun:test";
import { FileSystem } from "@effect/platform";
import type { PlatformError } from "@effect/platform/Error";
import { Effect, Layer } from "effect";
import {
  type CalendarEvent,
  classifyImpact,
  fetchCalendar,
  goldDirection,
  isGoldRelevant,
  type NewsImpact,
} from "./news.ts";

/** Même pattern que config.test.ts : `FileSystem` remplacé par une chaîne en mémoire, pour tester
 * la vraie logique de cache (readCache/writeCache, cf. docs/ARCHITECTURE.md) sans jamais toucher
 * ~/.aurum/calendar-cache.json. */
function fakeFileSystem(initial?: string) {
  let stored = initial;
  const fs = {
    readFileString: () =>
      stored === undefined
        ? Effect.fail(new Error("ENOENT") as unknown as PlatformError)
        : Effect.succeed(stored),
    writeFileString: (_path: string, data: string) =>
      Effect.sync(() => {
        stored = data;
      }),
  };
  const layer = Layer.succeed(FileSystem.FileSystem, fs as unknown as FileSystem.FileSystem);
  return {
    layer,
    get stored() {
      return stored;
    },
  };
}

function event(overrides: Partial<CalendarEvent>): CalendarEvent {
  return {
    title: "Some Indicator",
    country: "USD",
    date: "2026-07-29T12:30:00Z",
    impact: "High",
    forecast: "",
    previous: "",
    timestamp: 0,
    ...overrides,
  };
}

describe("classifyImpact", () => {
  test.each<[string, NewsImpact]>([
    ["High", "high"],
    [" medium ", "medium"],
    ["LOW", "low"],
    ["Holiday", "other"],
    ["", "other"],
  ])("classe %p en %p", (raw, expected) => {
    expect(classifyImpact(raw)).toBe(expected);
  });
});

describe("isGoldRelevant", () => {
  test("toute publication USD est pertinente", () => {
    expect(isGoldRelevant({ country: "USD", title: "Retail Sales m/m" })).toBe(true);
  });

  test("un titre mentionnant l'or est pertinent même hors USD", () => {
    expect(isGoldRelevant({ country: "EUR", title: "Central bank XAU reserves" })).toBe(true);
    expect(isGoldRelevant({ country: "CNY", title: "Gold import quota" })).toBe(true);
  });

  test("ni USD ni or ⇒ non pertinent", () => {
    expect(isGoldRelevant({ country: "EUR", title: "German ZEW Survey" })).toBe(false);
  });
});

describe("goldDirection", () => {
  test("indicateur pro-USD classique, prévision > précédent ⇒ baissier pour l'or", () => {
    const e = event({ title: "Non-Farm Payrolls", forecast: "200K", previous: "180K" });
    expect(goldDirection(e)).toBe("down");
  });

  test("indicateur pro-USD classique, prévision < précédent ⇒ haussier pour l'or", () => {
    const e = event({ title: "Retail Sales m/m", forecast: "0.2%", previous: "0.5%" });
    expect(goldDirection(e)).toBe("up");
  });

  test("indicateur inversé (chômage), prévision > précédent ⇒ haussier pour l'or", () => {
    const e = event({ title: "Unemployment Claims", forecast: "230K", previous: "210K" });
    expect(goldDirection(e)).toBe("up");
  });

  test("prévision === précédent ⇒ flat", () => {
    const e = event({ title: "CPI m/m", forecast: "0.3%", previous: "0.3%" });
    expect(goldDirection(e)).toBe("flat");
  });

  test("chiffres non parsables ⇒ undefined", () => {
    const e = event({ title: "FOMC Statement", forecast: "", previous: "" });
    expect(goldDirection(e)).toBeUndefined();
  });

  test("gère les suffixes K/M/B", () => {
    const e = event({ title: "Trade Balance", forecast: "1.2M", previous: "800K" });
    expect(goldDirection(e)).toBe("down");
  });
});

describe("fetchCalendar (cache, via le service FileSystem)", () => {
  test("cache du même jour, sans force ⇒ renvoie le cache tel quel, aucun appel réseau", async () => {
    // Le seul chemin testable sans mocker fetch() : un cache same-day fait retourner
    // fetchCalendar avant même d'atteindre fetchWithRetry (cf. news.ts#fetchCalendar).
    const cachedEvents: CalendarEvent[] = [event({ title: "Cached NFP" })];
    const { layer } = fakeFileSystem(
      JSON.stringify({ fetchedAt: new Date().toISOString(), events: cachedEvents }),
    );
    const result = await Effect.runPromise(Effect.provide(fetchCalendar(), layer));
    expect(result).toEqual(cachedEvents);
  });

  /** Coupe le réseau le temps de `run` — pour les scénarios "cache absent/inexploitable" ci-dessous,
   * où fetchCalendar doit retomber sur fetchWithRetry sans jamais faire un vrai appel réseau. */
  async function withNetworkDown(run: () => unknown): Promise<void> {
    const originalFetch = globalThis.fetch;
    globalThis.fetch = (() =>
      Promise.reject(new Error("réseau coupé (test)"))) as unknown as typeof globalThis.fetch;
    try {
      await run();
    } finally {
      globalThis.fetch = originalFetch;
    }
  }

  test("cache corrompu (JSON invalide) ⇒ ignoré silencieusement, retombe sur le réseau", async () => {
    const { layer } = fakeFileSystem("{ not valid json");
    await withNetworkDown(() =>
      expect(Effect.runPromise(Effect.provide(fetchCalendar(), layer))).rejects.toThrow(),
    );
  });

  test("aucun cache (fichier absent) ⇒ readCache renvoie undefined sans planter, retombe sur le réseau", async () => {
    const { layer } = fakeFileSystem(undefined);
    await withNetworkDown(() =>
      expect(Effect.runPromise(Effect.provide(fetchCalendar(), layer))).rejects.toThrow(),
    );
  });
});
