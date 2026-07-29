import { describe, expect, test } from "bun:test";
import { computeCotSnapshot, computeFredSnapshot } from "./macro.ts";

describe("computeCotSnapshot", () => {
  test("calcule la position nette et sa variation vs le rapport précédent", () => {
    const rows = [
      {
        report_date_as_yyyy_mm_dd: "2026-07-21",
        noncomm_positions_long_all: 224785,
        noncomm_positions_short_all: 40875,
        open_interest_all: 383368,
      },
      {
        report_date_as_yyyy_mm_dd: "2026-07-14",
        noncomm_positions_long_all: 220000,
        noncomm_positions_short_all: 45000,
        open_interest_all: 380000,
      },
    ];

    const snapshot = computeCotSnapshot(rows);

    expect(snapshot).toEqual({
      reportDate: "2026-07-21",
      net: 224785 - 40875,
      change: 224785 - 40875 - (220000 - 45000),
      openInterest: 383368,
    });
  });

  test("aucune ligne ⇒ undefined", () => {
    expect(computeCotSnapshot([])).toBeUndefined();
  });

  test("une seule ligne ⇒ pas de variation calculable", () => {
    const snapshot = computeCotSnapshot([
      {
        report_date_as_yyyy_mm_dd: "2026-07-21",
        noncomm_positions_long_all: 100,
        noncomm_positions_short_all: 40,
        open_interest_all: 500,
      },
    ]);
    expect(snapshot?.net).toBe(60);
    expect(snapshot?.change).toBeUndefined();
  });
});

describe("computeFredSnapshot", () => {
  test("prend la dernière observation valide et calcule la variation", () => {
    const snapshot = computeFredSnapshot([
      { date: "2026-07-28", value: "99.42" },
      { date: "2026-07-27", value: "99.60" },
    ]);
    expect(snapshot).toEqual({ date: "2026-07-28", value: 99.42, change: 99.42 - 99.6 });
  });

  test("ignore les observations marquées '.' (jour férié / donnée manquante)", () => {
    const snapshot = computeFredSnapshot([
      { date: "2026-07-28", value: "." },
      { date: "2026-07-27", value: "99.60" },
      { date: "2026-07-24", value: "99.80" },
    ]);
    expect(snapshot).toEqual({ date: "2026-07-27", value: 99.6, change: 99.6 - 99.8 });
  });

  test("aucune observation valide ⇒ undefined", () => {
    expect(computeFredSnapshot([{ date: "2026-07-28", value: "." }])).toBeUndefined();
    expect(computeFredSnapshot([])).toBeUndefined();
  });

  test("une seule observation valide ⇒ pas de variation calculable", () => {
    const snapshot = computeFredSnapshot([{ date: "2026-07-28", value: "1.85" }]);
    expect(snapshot).toEqual({ date: "2026-07-28", value: 1.85, change: undefined });
  });
});
