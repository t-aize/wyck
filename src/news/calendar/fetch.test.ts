import { afterEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { FileSystem } from "@effect/platform";
import { Effect } from "effect";
import { CACHE_FILE } from "./cache.ts";
import {
  CALENDAR_CACHE_TTL_MS,
  CALENDAR_URL,
  clampRetryAfter,
  fetchCalendar,
  isCacheFresh,
} from "./fetch.ts";
import { decorateEvents, parseCacheFile } from "./schemas.ts";

const sampleEvent = {
  title: "CPI y/y",
  country: "USD",
  date: "2026-09-11T08:30:00-04:00",
  impact: "High",
  forecast: "3.4%",
  previous: "3.4%",
};

function memoryFs(initial?: Record<string, string>) {
  const files = new Map<string, string>(Object.entries(initial ?? {}));
  const dirs = new Set<string>();
  return FileSystem.layerNoop({
    exists: (path) => Effect.succeed(files.has(path) || dirs.has(path)),
    readFileString: (path) => {
      const raw = files.get(path);
      return raw === undefined
        ? Effect.fail({
            _tag: "SystemError",
            module: "FileSystem",
            method: "readFileString",
            reason: "NotFound",
            pathOrDescriptor: path,
          } as never)
        : Effect.succeed(raw);
    },
    writeFileString: (path, data) =>
      Effect.sync(() => {
        files.set(path, data);
      }),
    makeDirectory: (path) =>
      Effect.sync(() => {
        dirs.add(path);
      }),
  });
}

function runFetch(options: { cacheDir: string; force?: boolean }, fs = memoryFs()) {
  return Effect.runPromise(fetchCalendar(options).pipe(Effect.provide(fs)));
}

const originalFetch = globalThis.fetch;

afterEach(() => {
  globalThis.fetch = originalFetch;
});

describe("clampRetryAfter", () => {
  test("defaults and caps", () => {
    expect(clampRetryAfter(null)).toBe(5);
    expect(clampRetryAfter("Wed, 21 Oct 2015 07:28:00 GMT")).toBe(5);
    expect(clampRetryAfter("0")).toBe(5);
    expect(clampRetryAfter("3600")).toBe(30);
    expect(clampRetryAfter("0.01")).toBe(0.01);
  });
});

describe("isCacheFresh", () => {
  test("respects TTL, not the calendar day", () => {
    const now = Date.parse("2026-09-10T18:00:00Z");
    expect(isCacheFresh(new Date(now - CALENDAR_CACHE_TTL_MS + 1_000).toISOString(), now)).toBe(
      true,
    );
    expect(isCacheFresh(new Date(now - CALENDAR_CACHE_TTL_MS - 1_000).toISOString(), now)).toBe(
      false,
    );
    expect(isCacheFresh("not-a-date", now)).toBe(false);
  });
});

describe("decorateEvents", () => {
  test("skips a malformed event and keeps the rest", () => {
    const events = decorateEvents([
      sampleEvent,
      { title: "Broken", country: "USD" },
      { ...sampleEvent, title: "NFP", date: "not-a-date" },
      { ...sampleEvent, title: "PPI m/m", forecast: null, previous: 0.2 },
    ]);
    expect(events?.map((event) => event.title)).toEqual(["CPI y/y", "PPI m/m"]);
    expect(events?.[1]?.forecast).toBe("");
    expect(events?.[1]?.previous).toBe("0.2");
  });

  test("rejects a non-array payload", () => {
    expect(decorateEvents({ title: "nope" })).toBeUndefined();
  });
});

describe("parseCacheFile", () => {
  test("drops NaN timestamps without discarding the file", () => {
    const parsed = parseCacheFile({
      fetchedAt: "2026-09-10T12:00:00.000Z",
      events: [
        { ...sampleEvent, timestamp: Number.NaN },
        { ...sampleEvent, title: "PPI m/m", timestamp: Date.parse(sampleEvent.date) },
      ],
    });
    expect(parsed?.events.map((event) => event.title)).toEqual(["CPI y/y", "PPI m/m"]);
  });
});

describe("fetchCalendar", () => {
  test("fetches, writes cache, and serves TTL hits without a second network call", async () => {
    let calls = 0;
    globalThis.fetch = (async (input: string | URL | Request) => {
      calls += 1;
      expect(String(input)).toBe(CALENDAR_URL);
      return new Response(JSON.stringify([sampleEvent]), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }) as unknown as typeof fetch;

    const fs = memoryFs();
    const first = await runFetch({ cacheDir: "/tmp/aurum" }, fs);
    const second = await runFetch({ cacheDir: "/tmp/aurum" }, fs);
    expect(first).toHaveLength(1);
    expect(second).toEqual(first);
    expect(calls).toBe(1);
  });

  test("force bypasses a fresh cache, stale cache is a network fallback", async () => {
    let calls = 0;
    globalThis.fetch = (async () => {
      calls += 1;
      if (calls === 1) {
        return new Response("nope", { status: 500 });
      }
      return new Response(JSON.stringify([sampleEvent]), { status: 200 });
    }) as unknown as typeof fetch;

    const stale = {
      fetchedAt: new Date(Date.now() - CALENDAR_CACHE_TTL_MS - 1_000).toISOString(),
      events: [{ ...sampleEvent, title: "Cached NFP", timestamp: Date.parse(sampleEvent.date) }],
    };
    const fs = memoryFs({ [join("/tmp/aurum", CACHE_FILE)]: JSON.stringify(stale) });

    const fallback = await runFetch({ cacheDir: "/tmp/aurum" }, fs);
    expect(fallback[0]?.title).toBe("Cached NFP");
    expect(calls).toBe(1);

    const forced = await runFetch({ cacheDir: "/tmp/aurum", force: true }, fs);
    expect(forced[0]?.title).toBe("CPI y/y");
    expect(calls).toBe(2);
  });

  test("retries 429 then succeeds", async () => {
    let calls = 0;
    globalThis.fetch = (async () => {
      calls += 1;
      if (calls === 1) {
        return new Response("", { status: 429, headers: { "retry-after": "0.01" } });
      }
      return new Response(JSON.stringify([sampleEvent]), { status: 200 });
    }) as unknown as typeof fetch;

    const events = await runFetch({ cacheDir: "/tmp/aurum" });
    expect(events).toHaveLength(1);
    expect(calls).toBe(2);
  });

  test("fails without cache on a non-array body", async () => {
    globalThis.fetch = (async () =>
      new Response(JSON.stringify({ error: "nope" }), { status: 200 })) as unknown as typeof fetch;

    const result = await Effect.runPromise(
      Effect.either(fetchCalendar({ cacheDir: "/tmp/aurum" }).pipe(Effect.provide(memoryFs()))),
    );
    expect(result._tag).toBe("Left");
    if (result._tag === "Left") {
      expect(result.left.message).toContain("pas un tableau");
    }
  });
});
