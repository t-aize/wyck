import { describe, expect, test } from "bun:test";
import type { MacroSnapshot } from "../macro.ts";
import type { CalendarEvent } from "../news.ts";
import { computeOverallBias } from "./bias.ts";
import type { StructureRow, StructureSnapshot } from "./structure.ts";

const NOW = new Date("2026-07-29T12:00:00Z");
const NOW_MS = NOW.getTime();

function row(label: string, overrides: Partial<StructureSnapshot> = {}): StructureRow {
  return {
    label,
    snapshot: {
      swingHigh: undefined,
      swingLow: undefined,
      bias: 0,
      signalType: undefined,
      signalDir: undefined,
      sweepLow: false,
      sweepHigh: false,
      ...overrides,
    },
  };
}

function event(minutesFromNow: number, overrides: Partial<CalendarEvent> = {}): CalendarEvent {
  return {
    title: "Non-Farm Payrolls",
    country: "USD",
    date: "",
    impact: "High",
    forecast: "",
    previous: "",
    timestamp: NOW_MS + minutesFromNow * 60_000,
    ...overrides,
  };
}

describe("computeOverallBias — ancrage/direction", () => {
  test("rows undefined ⇒ pas de biais", () => {
    const result = computeOverallBias(undefined, undefined, [], NOW);
    expect(result.anchor).toBeUndefined();
    expect(result.direction).toBe(0);
    expect(result.anchorConflict).toBe(false);
    expect(result.conviction).toBeUndefined();
  });

  test("rows vide ⇒ pas de biais", () => {
    const result = computeOverallBias([], undefined, [], NOW);
    expect(result.anchor).toBeUndefined();
    expect(result.direction).toBe(0);
  });

  test("étiquette D1 manquante ⇒ pas d'ancrage", () => {
    const rows = [row("15M"), row("1H"), row("4H", { bias: 1 }), row("5M")];
    const result = computeOverallBias(rows, undefined, [], NOW);
    expect(result.anchor).toBeUndefined();
    expect(result.direction).toBe(0);
  });

  test("D1=4H=haussier ⇒ direction haussière, pas de conflit", () => {
    const rows = [row("D1", { bias: 1 }), row("4H", { bias: 1 })];
    const result = computeOverallBias(rows, undefined, [], NOW);
    expect(result.anchor).toEqual({ d1: 1, h4: 1 });
    expect(result.direction).toBe(1);
    expect(result.anchorConflict).toBe(false);
  });

  test("D1=4H=baissier ⇒ direction baissière, pas de conflit", () => {
    const rows = [row("D1", { bias: -1 }), row("4H", { bias: -1 })];
    const result = computeOverallBias(rows, undefined, [], NOW);
    expect(result.direction).toBe(-1);
    expect(result.anchorConflict).toBe(false);
  });

  test("D1 et 4H opposés (tous deux directionnels) ⇒ pas de biais, conflit réel signalé", () => {
    const rows = [row("D1", { bias: 1 }), row("4H", { bias: -1 })];
    const result = computeOverallBias(rows, undefined, [], NOW);
    expect(result.anchor).toEqual({ d1: 1, h4: -1 });
    expect(result.direction).toBe(0);
    expect(result.anchorConflict).toBe(true);
  });

  test("un des deux neutre ⇒ pas de biais, mais pas un conflit (cas banal)", () => {
    const rows = [row("D1", { bias: 1 }), row("4H", { bias: 0 })];
    const result = computeOverallBias(rows, undefined, [], NOW);
    expect(result.direction).toBe(0);
    expect(result.anchorConflict).toBe(false);
  });

  test("les deux neutres ⇒ pas de biais, pas un conflit", () => {
    const rows = [row("D1", { bias: 0 }), row("4H", { bias: 0 })];
    const result = computeOverallBias(rows, undefined, [], NOW);
    expect(result.direction).toBe(0);
    expect(result.anchorConflict).toBe(false);
  });
});

describe("computeOverallBias — repli 1H (D1/4H tous les deux neutres)", () => {
  test("D1/4H neutres, 1H haussier, rien d'autre ⇒ repli défini, faible", () => {
    const rows = [row("D1", { bias: 0 }), row("4H", { bias: 0 }), row("1H", { bias: 1 })];
    const result = computeOverallBias(rows, undefined, [], NOW);
    expect(result.secondary).toEqual({
      direction: 1,
      confirmations: [
        { label: "15M", biasMatch: false, signalMatch: false, sweepMatch: false, confirms: false },
        { label: "5M", biasMatch: false, signalMatch: false, sweepMatch: false, confirms: false },
      ],
      confirmationCount: 0,
      macroFactors: [
        { factor: "cot", alignment: "unavailable", change: undefined },
        { factor: "usdBroad", alignment: "unavailable", change: undefined },
        { factor: "realYield", alignment: "unavailable", change: undefined },
      ],
      macroAlignedCount: 0,
      macroAgainstCount: 0,
      conviction: "weak",
    });
  });

  test("D1/4H neutres, 1H haussier, 15M confirme ⇒ conviction modérée", () => {
    const rows = [
      row("D1", { bias: 0 }),
      row("4H", { bias: 0 }),
      row("1H", { bias: 1 }),
      row("15M", { bias: 1 }),
    ];
    const result = computeOverallBias(rows, undefined, [], NOW);
    expect(result.secondary?.confirmationCount).toBe(1);
    expect(result.secondary?.conviction).toBe("moderate");
  });

  test("D1/4H neutres, 1H lui-même neutre ⇒ pas de repli (rien à proposer)", () => {
    const rows = [row("D1", { bias: 0 }), row("4H", { bias: 0 }), row("1H", { bias: 0 })];
    const result = computeOverallBias(rows, undefined, [], NOW);
    expect(result.secondary).toBeUndefined();
  });

  test("D1/4H neutres, 1H absent ⇒ pas de repli", () => {
    const rows = [row("D1", { bias: 0 }), row("4H", { bias: 0 })];
    const result = computeOverallBias(rows, undefined, [], NOW);
    expect(result.secondary).toBeUndefined();
  });

  test("D1 directionnel ⇒ pas de repli, même si 4H neutre", () => {
    const rows = [row("D1", { bias: 1 }), row("4H", { bias: 0 }), row("1H", { bias: 1 })];
    const result = computeOverallBias(rows, undefined, [], NOW);
    expect(result.secondary).toBeUndefined();
  });

  test("D1/4H en conflit réel ⇒ pas de repli non plus (pas le même cas)", () => {
    const rows = [row("D1", { bias: 1 }), row("4H", { bias: -1 }), row("1H", { bias: 1 })];
    const result = computeOverallBias(rows, undefined, [], NOW);
    expect(result.anchorConflict).toBe(true);
    expect(result.secondary).toBeUndefined();
  });

  test("D1=4H directionnels (biais primaire déjà clair) ⇒ pas de repli", () => {
    const rows = [row("D1", { bias: 1 }), row("4H", { bias: 1 }), row("1H", { bias: -1 })];
    const result = computeOverallBias(rows, undefined, [], NOW);
    expect(result.direction).toBe(1);
    expect(result.secondary).toBeUndefined();
  });

  test("macro alignée bonifie aussi la conviction du repli", () => {
    const rows = [row("D1", { bias: 0 }), row("4H", { bias: 0 }), row("1H", { bias: 1 })];
    const macro: MacroSnapshot = {
      cot: { reportDate: "", net: 0, change: 1000, openInterest: 0 },
      usdBroad: { date: "", value: 0, change: -0.5 },
    };
    const result = computeOverallBias(rows, macro, [], NOW);
    expect(result.secondary?.macroAlignedCount).toBe(2);
    expect(result.secondary?.conviction).toBe("moderate"); // base faible (0 confirmation) + bump macro
  });
});

describe("computeOverallBias — confirmation (1H/15M/5M)", () => {
  test("confirme via le biais", () => {
    const rows = [row("D1", { bias: 1 }), row("4H", { bias: 1 }), row("1H", { bias: 1 })];
    const result = computeOverallBias(rows, undefined, [], NOW);
    const oneHour = result.confirmations.find((c) => c.label === "1H")!;
    expect(oneHour).toEqual({
      label: "1H",
      biasMatch: true,
      signalMatch: false,
      sweepMatch: false,
      confirms: true,
    });
    expect(result.confirmationCount).toBe(1);
  });

  test("confirme via le signal (BOS/CHoCH) même si le biais ne correspond pas", () => {
    const rows = [
      row("D1", { bias: 1 }),
      row("4H", { bias: 1 }),
      row("15M", { bias: -1, signalType: "BOS", signalDir: 1 }),
    ];
    const result = computeOverallBias(rows, undefined, [], NOW);
    const fifteenMin = result.confirmations.find((c) => c.label === "15M")!;
    expect(fifteenMin.biasMatch).toBe(false);
    expect(fifteenMin.signalMatch).toBe(true);
    expect(fifteenMin.confirms).toBe(true);
    expect(result.confirmationCount).toBe(1);
  });

  test("confirme via le sweep", () => {
    const rows = [row("D1", { bias: 1 }), row("4H", { bias: 1 }), row("5M", { sweepLow: true })];
    const result = computeOverallBias(rows, undefined, [], NOW);
    const fiveMin = result.confirmations.find((c) => c.label === "5M")!;
    expect(fiveMin.sweepMatch).toBe(true);
    expect(fiveMin.confirms).toBe(true);
  });

  test("tout dans le mauvais sens ⇒ ne confirme pas", () => {
    const rows = [
      row("D1", { bias: 1 }),
      row("4H", { bias: 1 }),
      row("1H", { bias: -1, signalType: "CHoCH", signalDir: -1, sweepHigh: true }),
    ];
    const result = computeOverallBias(rows, undefined, [], NOW);
    const oneHour = result.confirmations.find((c) => c.label === "1H")!;
    expect(oneHour.confirms).toBe(false);
  });

  test("aucun des trois TF présent ⇒ confirmationCount 0", () => {
    const rows = [row("D1", { bias: 1 }), row("4H", { bias: 1 })];
    const result = computeOverallBias(rows, undefined, [], NOW);
    expect(result.confirmationCount).toBe(0);
    expect(result.confirmations.every((c) => !c.confirms)).toBe(true);
  });

  test("les trois confirment ⇒ confirmationCount 3", () => {
    const rows = [
      row("D1", { bias: 1 }),
      row("4H", { bias: 1 }),
      row("1H", { bias: 1 }),
      row("15M", { sweepLow: true }),
      row("5M", { signalType: "BOS", signalDir: 1 }),
    ];
    const result = computeOverallBias(rows, undefined, [], NOW);
    expect(result.confirmationCount).toBe(3);
  });

  test("un TF manquant (15M absent) ⇒ traité comme non confirmant, pas d'erreur", () => {
    const rows = [
      row("D1", { bias: 1 }),
      row("4H", { bias: 1 }),
      row("1H", { bias: 1 }),
      row("5M", { bias: 1 }),
    ];
    const result = computeOverallBias(rows, undefined, [], NOW);
    const fifteenMin = result.confirmations.find((c) => c.label === "15M")!;
    expect(fifteenMin.confirms).toBe(false);
    expect(result.confirmationCount).toBe(2);
  });

  test("cas symétrique baissier : sweepHigh confirme", () => {
    const rows = [
      row("D1", { bias: -1 }),
      row("4H", { bias: -1 }),
      row("15M", { sweepHigh: true }),
    ];
    const result = computeOverallBias(rows, undefined, [], NOW);
    const fifteenMin = result.confirmations.find((c) => c.label === "15M")!;
    expect(fifteenMin.sweepMatch).toBe(true);
    expect(fifteenMin.confirms).toBe(true);
  });
});

describe("computeOverallBias — alignement macro", () => {
  const bullishRows = [row("D1", { bias: 1 }), row("4H", { bias: 1 })];
  const bearishRows = [row("D1", { bias: -1 }), row("4H", { bias: -1 })];

  function macroWith(overrides: Partial<MacroSnapshot>): MacroSnapshot {
    return { ...overrides };
  }

  test("COT en hausse ⇒ aligné (haussier)", () => {
    const macro = macroWith({ cot: { reportDate: "", net: 0, change: 5000, openInterest: 0 } });
    const result = computeOverallBias(bullishRows, macro, [], NOW);
    expect(result.macroFactors.find((f) => f.factor === "cot")?.alignment).toBe("aligned");
  });

  test("COT en baisse ⇒ contre (haussier)", () => {
    const macro = macroWith({ cot: { reportDate: "", net: 0, change: -5000, openInterest: 0 } });
    const result = computeOverallBias(bullishRows, macro, [], NOW);
    expect(result.macroFactors.find((f) => f.factor === "cot")?.alignment).toBe("against");
  });

  test("COT inchangé ⇒ neutre", () => {
    const macro = macroWith({ cot: { reportDate: "", net: 0, change: 0, openInterest: 0 } });
    const result = computeOverallBias(bullishRows, macro, [], NOW);
    expect(result.macroFactors.find((f) => f.factor === "cot")?.alignment).toBe("neutral");
  });

  test("COT absent ⇒ indisponible", () => {
    const result = computeOverallBias(bullishRows, macroWith({}), [], NOW);
    expect(result.macroFactors.find((f) => f.factor === "cot")?.alignment).toBe("unavailable");
  });

  test("dollar large en baisse ⇒ aligné (haussier)", () => {
    const macro = macroWith({ usdBroad: { date: "", value: 0, change: -0.5 } });
    const result = computeOverallBias(bullishRows, macro, [], NOW);
    expect(result.macroFactors.find((f) => f.factor === "usdBroad")?.alignment).toBe("aligned");
  });

  test("dollar large en hausse ⇒ contre (haussier)", () => {
    const macro = macroWith({ usdBroad: { date: "", value: 0, change: 0.5 } });
    const result = computeOverallBias(bullishRows, macro, [], NOW);
    expect(result.macroFactors.find((f) => f.factor === "usdBroad")?.alignment).toBe("against");
  });

  test("taux réel en baisse ⇒ aligné (haussier)", () => {
    const macro = macroWith({ realYield: { date: "", value: 0, change: -0.05 } });
    const result = computeOverallBias(bullishRows, macro, [], NOW);
    expect(result.macroFactors.find((f) => f.factor === "realYield")?.alignment).toBe("aligned");
  });

  test("taux réel en hausse ⇒ contre (haussier)", () => {
    const macro = macroWith({ realYield: { date: "", value: 0, change: 0.05 } });
    const result = computeOverallBias(bullishRows, macro, [], NOW);
    expect(result.macroFactors.find((f) => f.factor === "realYield")?.alignment).toBe("against");
  });

  test("macro totalement absent ⇒ les 3 indisponibles, aucun compte", () => {
    const result = computeOverallBias(bullishRows, undefined, [], NOW);
    expect(result.macroFactors.every((f) => f.alignment === "unavailable")).toBe(true);
    expect(result.macroAlignedCount).toBe(0);
    expect(result.macroAgainstCount).toBe(0);
  });

  test("mélange : 1 aligné, 1 contre, 1 indisponible", () => {
    const macro = macroWith({
      cot: { reportDate: "", net: 0, change: 1000, openInterest: 0 },
      usdBroad: { date: "", value: 0, change: 0.5 },
    });
    const result = computeOverallBias(bullishRows, macro, [], NOW);
    expect(result.macroAlignedCount).toBe(1);
    expect(result.macroAgainstCount).toBe(1);
  });

  test("les 3 alignés", () => {
    const macro = macroWith({
      cot: { reportDate: "", net: 0, change: 1000, openInterest: 0 },
      usdBroad: { date: "", value: 0, change: -0.5 },
      realYield: { date: "", value: 0, change: -0.05 },
    });
    const result = computeOverallBias(bullishRows, macro, [], NOW);
    expect(result.macroAlignedCount).toBe(3);
  });

  test("inversion de signe (baissier) : COT en baisse et dollar/taux en hausse sont alignés", () => {
    const macro = macroWith({
      cot: { reportDate: "", net: 0, change: -1000, openInterest: 0 },
      usdBroad: { date: "", value: 0, change: 0.5 },
      realYield: { date: "", value: 0, change: 0.05 },
    });
    const result = computeOverallBias(bearishRows, macro, [], NOW);
    expect(result.macroAlignedCount).toBe(3);
  });
});

describe("computeOverallBias — conviction", () => {
  function rowsWithConfirmationCount(count: 0 | 1 | 2 | 3) {
    const rows = [row("D1", { bias: 1 }), row("4H", { bias: 1 })];
    if (count >= 1) rows.push(row("1H", { bias: 1 }));
    if (count >= 2) rows.push(row("15M", { bias: 1 }));
    if (count >= 3) rows.push(row("5M", { bias: 1 }));
    return rows;
  }

  const macroTwoAligned: MacroSnapshot = {
    cot: { reportDate: "", net: 0, change: 1000, openInterest: 0 },
    usdBroad: { date: "", value: 0, change: -0.5 },
  };
  const macroThreeAgainst: MacroSnapshot = {
    cot: { reportDate: "", net: 0, change: -1000, openInterest: 0 },
    usdBroad: { date: "", value: 0, change: 0.5 },
    realYield: { date: "", value: 0, change: 0.05 },
  };

  test("0 confirmation ⇒ faible", () => {
    const result = computeOverallBias(rowsWithConfirmationCount(0), undefined, [], NOW);
    expect(result.conviction).toBe("weak");
  });

  test("1 confirmation ⇒ modérée", () => {
    const result = computeOverallBias(rowsWithConfirmationCount(1), undefined, [], NOW);
    expect(result.conviction).toBe("moderate");
  });

  test("2 confirmations ⇒ forte", () => {
    const result = computeOverallBias(rowsWithConfirmationCount(2), undefined, [], NOW);
    expect(result.conviction).toBe("strong");
  });

  test("3 confirmations ⇒ forte (pas de palier au-delà)", () => {
    const result = computeOverallBias(rowsWithConfirmationCount(3), undefined, [], NOW);
    expect(result.conviction).toBe("strong");
  });

  test("0 confirmation + 2 facteurs macro alignés ⇒ bump borné à modérée, pas forte", () => {
    const result = computeOverallBias(rowsWithConfirmationCount(0), macroTwoAligned, [], NOW);
    expect(result.conviction).toBe("moderate");
  });

  test("2 confirmations (forte) + macro contre ⇒ ramenée à modérée", () => {
    const result = computeOverallBias(rowsWithConfirmationCount(2), macroThreeAgainst, [], NOW);
    expect(result.conviction).toBe("moderate");
  });

  test("1 confirmation (modérée) + macro contre ⇒ ramenée à faible", () => {
    const result = computeOverallBias(rowsWithConfirmationCount(1), macroThreeAgainst, [], NOW);
    expect(result.conviction).toBe("weak");
  });

  test("0 confirmation (faible) + macro contre ⇒ reste faible (plancher)", () => {
    const result = computeOverallBias(rowsWithConfirmationCount(0), macroThreeAgainst, [], NOW);
    expect(result.conviction).toBe("weak");
  });

  test("ancrage en désaccord ⇒ pas de conviction ni de confirmation/macro calculés, même avec des entrées qui auraient l'air favorables", () => {
    const rows = [
      row("D1", { bias: 1 }),
      row("4H", { bias: -1 }),
      row("1H", { bias: 1 }),
      row("15M", { bias: 1 }),
      row("5M", { bias: 1 }),
    ];
    const result = computeOverallBias(rows, macroTwoAligned, [], NOW);
    expect(result.direction).toBe(0);
    expect(result.conviction).toBeUndefined();
    expect(result.confirmationCount).toBe(0);
    expect(result.confirmations).toEqual([]);
    expect(result.macroFactors).toEqual([]);
  });
});

describe("computeOverallBias — avertissement calendrier", () => {
  test("événement dans 30 minutes ⇒ avertissement", () => {
    const result = computeOverallBias([], undefined, [event(30)], NOW);
    expect(result.caution?.minutesUntil).toBe(30);
  });

  test("événement dans 91 minutes ⇒ hors fenêtre", () => {
    const result = computeOverallBias([], undefined, [event(91)], NOW);
    expect(result.caution).toBeUndefined();
  });

  test("événement pile à 90 minutes ⇒ borne incluse", () => {
    const result = computeOverallBias([], undefined, [event(90)], NOW);
    expect(result.caution?.minutesUntil).toBe(90);
  });

  test("événement passé (il y a 10 minutes) ⇒ exclu", () => {
    const result = computeOverallBias([], undefined, [event(-10)], NOW);
    expect(result.caution).toBeUndefined();
  });

  test("impact non élevé ⇒ exclu", () => {
    const result = computeOverallBias([], undefined, [event(30, { impact: "Medium" })], NOW);
    expect(result.caution).toBeUndefined();
  });

  test("non pertinent pour l'or ⇒ exclu", () => {
    const result = computeOverallBias(
      [],
      undefined,
      [event(30, { country: "JPY", title: "Bank of Japan Rate Decision" })],
      NOW,
    );
    expect(result.caution).toBeUndefined();
  });

  test("plusieurs événements ⇒ le plus proche est retenu", () => {
    const result = computeOverallBias([], undefined, [event(60), event(20)], NOW);
    expect(result.caution?.minutesUntil).toBe(20);
  });

  test("présent même sans biais clair (ancrage en désaccord)", () => {
    const rows = [row("D1", { bias: 1 }), row("4H", { bias: -1 })];
    const result = computeOverallBias(rows, undefined, [event(30)], NOW);
    expect(result.direction).toBe(0);
    expect(result.caution?.minutesUntil).toBe(30);
  });
});

describe("computeOverallBias — scénario complet", () => {
  test("confluence haussière partielle avec macro mitigé et news imminente", () => {
    const rows = [
      row("D1", { bias: 1 }),
      row("4H", { bias: 1 }),
      row("1H", { bias: 1 }),
      row("15M", { sweepLow: true }),
      row("5M"), // ne confirme pas
    ];
    const macro: MacroSnapshot = {
      cot: { reportDate: "2026-07-25", net: 180000, change: 5000, openInterest: 400000 },
      usdBroad: { date: "2026-07-28", value: 120.5, change: -0.3 },
      realYield: { date: "2026-07-28", value: 1.9, change: 0.05 },
    };
    const calendar = [event(20)];

    const result = computeOverallBias(rows, macro, calendar, NOW);

    expect(result.direction).toBe(1);
    expect(result.anchor).toEqual({ d1: 1, h4: 1 });
    expect(result.confirmationCount).toBe(2);
    expect(result.macroAlignedCount).toBe(2);
    expect(result.macroAgainstCount).toBe(1);
    expect(result.conviction).toBe("strong");
    expect(result.caution?.minutesUntil).toBe(20);
  });
});
