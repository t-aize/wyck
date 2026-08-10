import { describe, expect, test } from "bun:test";
import { requestCapMs } from "./useTrend.ts";

describe("requestCapMs", () => {
  test("H1 (60min/bougie) : plafonné par la plage serveur (700h), pas par le nombre de bougies", () => {
    // 900 bougies × 60min = 900h > 700h ⇒ la plage l'emporte.
    expect(requestCapMs(60 * 60_000)).toBe(700 * 60 * 60_000);
  });

  test("M15 (15min/bougie) : plafonné par le nombre de bougies (900), pas par la plage", () => {
    // 900 × 15min = 225h < 700h ⇒ le nombre de bougies l'emporte.
    expect(requestCapMs(15 * 60_000)).toBe(900 * 15 * 60_000);
  });

  test("M5 (5min/bougie) : plafonné par le nombre de bougies, très en dessous de 700h", () => {
    // 900 × 5min = 75h, très loin des 700h — c'était le trou silencieux avant ce fix : une
    // fenêtre de 700h y contient ~8400 bougies, le serveur n'en renvoyait qu'un maximum d'environ
    // 1000, laissant le reste de la fenêtre absent des données sans erreur.
    const cap = requestCapMs(5 * 60_000);
    expect(cap).toBe(900 * 5 * 60_000);
    expect(cap).toBeLessThan(700 * 60 * 60_000);
  });
});
