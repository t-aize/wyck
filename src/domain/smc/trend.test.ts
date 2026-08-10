import { describe, expect, test } from "bun:test";
import type { CtraderTrendbar } from "../../ctrader/client.ts";
import {
  classifyEventTrend,
  classifyStructuralTrend,
  computeAdx,
  computeEmaStack,
  computeTrendState,
  detectLatestSweep,
  detectStructureEvents,
  type StructureEvent,
} from "./trend.ts";

/** Bougie synthétique. */
function bar(i: number, high: number, low: number, close = (high + low) / 2): CtraderTrendbar {
  return { timestamp: i * 60_000, open: (high + low) / 2, high, low, close, volume: 0 };
}

// Zigzag net, fractale left=right=2 (fenêtre de 5 bougies) : creux #1=75(idx2), sommet #1=120(idx7),
// creux #2=85(idx12, > 75 ⇒ HL), sommet #2=130(idx17, > 120 ⇒ HH). Séries tracées via l'algorithme
// réel (cf. commentaire équivalent dans les anciens tests), pas calculées à la main.
const ZIGZAG_UP: CtraderTrendbar[] = [
  bar(0, 90, 85),
  bar(1, 92, 87),
  bar(2, 80, 75),
  bar(3, 92, 87),
  bar(4, 94, 89),
  bar(5, 100, 95),
  bar(6, 110, 105),
  bar(7, 120, 115),
  bar(8, 108, 103),
  bar(9, 106, 101),
  bar(10, 100, 95),
  bar(11, 96, 91),
  bar(12, 90, 85),
  bar(13, 94, 89),
  bar(14, 98, 93),
  bar(15, 106, 101),
  bar(16, 116, 111),
  bar(17, 130, 125),
  bar(18, 120, 115),
  bar(19, 118, 113),
];

describe("classifyStructuralTrend", () => {
  test("HH + HL confirmés ⇒ structurel haussier", () => {
    expect(classifyStructuralTrend(ZIGZAG_UP, { left: 2, right: 2 })).toBe(1);
  });

  test("pas encore deux pivots de chaque côté ⇒ range (0)", () => {
    expect(classifyStructuralTrend(ZIGZAG_UP.slice(0, 8), { left: 2, right: 2 })).toBe(0);
  });

  test("minSwingPct filtre un swing trop proche du précédent", () => {
    // Même jambe haussière, mais le second sommet (idx17) est presque identique au premier (120 ⇒
    // 120.01, +0.008%) : sous minSwingPct=1%, il est ignoré comme "même niveau" — le HL du creux
    // suffit à établir un biais avec l'ancien sommet, sans filtre ; avec filtre, pas de second
    // sommet distinct ⇒ retombe en range.
    const bars: CtraderTrendbar[] = [
      ...ZIGZAG_UP.slice(0, 17),
      bar(17, 120.01, 115.01),
      bar(18, 108, 103),
      bar(19, 106, 101),
    ];
    expect(classifyStructuralTrend(bars, { left: 2, right: 2 })).toBe(1);
    expect(classifyStructuralTrend(bars, { left: 2, right: 2, minSwingPct: 1 })).toBe(0);
  });
});

describe("detectStructureEvents", () => {
  test("cassure sur clôture de corps (pas une mèche) dans le sens du trend ⇒ BOS", () => {
    // Sommet surveillé = 122 (idx17). Recul, puis clôture au-dessus (124 @ idx22) sur une bougie
    // distincte du sommet lui-même (évite la collision "le pivot qui casse son propre niveau").
    const bars: CtraderTrendbar[] = [
      ...ZIGZAG_UP.slice(0, 18).map((b, i) => (i === 17 ? bar(17, 122, 118) : b)),
      bar(18, 108, 103, 106),
      bar(19, 106, 101, 103),
      bar(20, 112, 107, 109),
      bar(21, 118, 113, 116),
      bar(22, 126, 121, 124),
    ];
    const { events } = detectStructureEvents(bars, { left: 2, right: 2, displacementMult: 0 });
    expect(events).toHaveLength(1);
    expect(events[0]).toMatchObject({ index: 22, price: 122, kind: "BOS", direction: 1 });
  });

  test("displacementMult élevé rejette une cassure de faible amplitude (métadonnée, pas un filtre dur)", () => {
    const bars: CtraderTrendbar[] = [
      ...ZIGZAG_UP.slice(0, 18).map((b, i) => (i === 17 ? bar(17, 122, 118) : b)),
      bar(18, 108, 103, 106),
      bar(19, 106, 101, 103),
      bar(20, 112, 107, 109),
      bar(21, 118, 113, 116),
      bar(22, 126, 121, 124),
    ];
    const { events } = detectStructureEvents(bars, { left: 2, right: 2, displacementMult: 100 });
    // L'événement est toujours détecté (displacement n'altère pas la détection elle-même)...
    expect(events).toHaveLength(1);
    // ...mais displacementOk reflète l'amplitude insuffisante pour un multiplicateur aussi exigeant.
    expect(events[0]!.displacementOk).toBe(false);
  });

  test("mèche plus haute qui ne clôture pas au-dessus ⇒ le sommet le plus PROCHE reste le niveau surveillé, pas le dernier formé", () => {
    // Sommet #1 = 100 (idx2). Plus tard, sommet #2 = 130 (idx8) formé par une simple mèche (clôture
    // 95, jamais au-dessus de 100 ni de 130) : un swing se détecte sur le high, pas sur la clôture,
    // donc #2 devient le "dernier swing formé" sans jamais casser #1 en clôture — les deux restent
    // valides. Le prix courant (~85) est plus proche de 100 que de 130 : c'est 100 qui doit rester
    // affiché en résistance, pas 130 simplement parce qu'il est plus récent.
    const bars: CtraderTrendbar[] = [
      bar(0, 90, 85),
      bar(1, 92, 87),
      bar(2, 100, 95), // sommet #1 = 100
      bar(3, 92, 87),
      bar(4, 90, 85),
      bar(5, 88, 83),
      bar(6, 85, 80),
      bar(7, 90, 85),
      bar(8, 130, 90, 95), // mèche à 130, clôture 95 (sommet #2 = 130, jamais cassé en clôture)
      bar(9, 90, 85),
      bar(10, 88, 83),
    ];
    const { pending } = detectStructureEvents(bars, { left: 2, right: 2 });
    expect(pending.resistance).toEqual({ level: 100, kind: "BOS" });
  });

  test("symétrique côté support : mèche plus basse qui ne clôture pas en dessous ⇒ le creux le plus proche reste surveillé", () => {
    const bars: CtraderTrendbar[] = [
      bar(0, 85, 80),
      bar(1, 83, 78),
      bar(2, 75, 70), // creux #1 = 70
      bar(3, 83, 78),
      bar(4, 85, 80),
      bar(5, 87, 82),
      bar(6, 89, 84),
      bar(7, 85, 80),
      bar(8, 90, 40, 75), // mèche à 40, clôture 75 (creux #2 = 40, jamais cassé en clôture)
      bar(9, 85, 80),
      bar(10, 87, 82),
    ];
    const { pending } = detectStructureEvents(bars, { left: 2, right: 2 });
    expect(pending.support).toEqual({ level: 70, kind: "BOS" });
  });
});

describe("classifyEventTrend", () => {
  // Événements construits à la main plutôt que dérivés de detectStructureEvents : la règle de
  // confirmation CHoCH→BOS est une fonction pure sur une liste d'événements, indépendante de la
  // façon dont ces événements ont été détectés (fractale ici, zigzag ailleurs) — testée isolément.
  function event(index: number, kind: StructureEvent["kind"], direction: 1 | -1): StructureEvent {
    return { index, price: 0, kind, direction, displacementOk: true };
  }

  test("premier événement (aucun trend établi) : un BOS confirme directement (bootstrap)", () => {
    const result = classifyEventTrend([event(0, "BOS", 1)]);
    expect(result).toEqual({ raw: 1, confirmed: 1, lastDisplacementOk: true });
  });

  test("un CHoCH seul inverse raw mais PAS confirmed — c'est tout le point de la règle", () => {
    const result = classifyEventTrend([event(0, "BOS", 1), event(1, "CHoCH", -1)]);
    expect(result.raw).toBe(-1);
    expect(result.confirmed).toBe(1); // toujours haussier : le CHoCH seul ne suffit pas
  });

  test("un BOS dans le même sens que le CHoCH précédent confirme enfin le retournement", () => {
    const result = classifyEventTrend([
      event(0, "BOS", 1),
      event(1, "CHoCH", -1),
      event(2, "BOS", -1),
    ]);
    expect(result).toEqual({ raw: -1, confirmed: -1, lastDisplacementOk: true });
  });

  test("aucun événement ⇒ tout à 0, lastDisplacementOk undefined", () => {
    expect(classifyEventTrend([])).toEqual({ raw: 0, confirmed: 0, lastDisplacementOk: undefined });
  });
});

describe("detectLatestSweep", () => {
  const setup: CtraderTrendbar[] = [
    bar(0, 90, 85),
    bar(1, 92, 87),
    bar(2, 80, 75), // creux surveillé = 75
    bar(3, 92, 87),
    bar(4, 94, 89),
    bar(5, 90, 85),
    bar(6, 88, 83),
  ];

  test("mèche sous le creux surveillé, clôture repassée au-dessus ⇒ sweepLow", () => {
    const bars = [...setup, bar(7, 84, 70, 82)];
    expect(detectLatestSweep(bars, { left: 2, right: 2 })).toEqual({
      sweepHigh: false,
      sweepLow: true,
    });
  });

  test("clôture qui reste sous le niveau ⇒ vraie cassure, pas un sweep", () => {
    const bars = [...setup, bar(7, 84, 70, 72)];
    expect(detectLatestSweep(bars, { left: 2, right: 2 })).toEqual({
      sweepHigh: false,
      sweepLow: false,
    });
  });
});

describe("computeEmaStack", () => {
  function closeBar(i: number, close: number): CtraderTrendbar {
    return { timestamp: i, open: close, high: close + 1, low: close - 1, close, volume: 0 };
  }

  test("tendance haussière régulière ⇒ EMA rapide > EMA lente", () => {
    const bars = Array.from({ length: 60 }, (_, i) => closeBar(i, 100 + i * 0.5));
    expect(computeEmaStack(bars, { fast: 10, slow: 30 })).toBe(true);
  });

  test("tendance baissière régulière ⇒ EMA rapide < EMA lente", () => {
    const bars = Array.from({ length: 60 }, (_, i) => closeBar(i, 200 - i * 0.5));
    expect(computeEmaStack(bars, { fast: 10, slow: 30 })).toBe(false);
  });

  test("pas assez de bougies pour l'EMA lente ⇒ undefined", () => {
    const bars = Array.from({ length: 10 }, (_, i) => closeBar(i, 100 + i));
    expect(computeEmaStack(bars, { fast: 10, slow: 30 })).toBeUndefined();
  });
});

describe("computeAdx", () => {
  test("pas assez de bougies ⇒ undefined", () => {
    const bars = Array.from({ length: 10 }, (_, i) => bar(i, 101, 99, 100));
    expect(computeAdx(bars, 14)).toBeUndefined();
  });

  test('tendance nette et régulière ⇒ ADX élevé (> 25, seuil usuel "en tendance")', () => {
    const bars = Array.from({ length: 40 }, (_, i) => {
      const base = 100 + i * 2;
      return bar(i, base + 1, base - 1, base + 0.5);
    });
    expect(computeAdx(bars, 14)!).toBeGreaterThan(25);
  });

  test("marché plat et bruité ⇒ ADX bas (< 25)", () => {
    let seed = 42;
    function rand() {
      seed = (seed * 1103515245 + 12345) & 0x7fffffff;
      return seed / 0x7fffffff;
    }
    const bars = Array.from({ length: 40 }, (_, i) => {
      const base = 100 + (rand() - 0.5) * 2;
      return bar(i, base + 1, base - 1, base);
    });
    expect(computeAdx(bars, 14)!).toBeLessThan(25);
  });
});

describe("computeTrendState", () => {
  test("historique trop court ⇒ état vide", () => {
    const bars = ZIGZAG_UP.slice(0, 3);
    expect(computeTrendState(bars, { left: 2, right: 2 })).toEqual({
      structural: 0,
      rawEvent: 0,
      confirmedEvent: 0,
      lastEventDisplacementOk: undefined,
      sweepHigh: false,
      sweepLow: false,
      emaStackBullish: undefined,
      adx: undefined,
      pending: { resistance: undefined, support: undefined },
    });
  });

  test("combine structural + événementiel sur un même historique haussier", () => {
    const state = computeTrendState(ZIGZAG_UP, {
      left: 2,
      right: 2,
      displacementMult: 0,
      emaFast: 3,
      emaSlow: 5,
      adxPeriod: 3,
    });
    expect(state.structural).toBe(1);
    expect(state.emaStackBullish).toBe(true);
    expect(state.adx).toBeGreaterThan(0);
  });

  test("expose le prochain niveau de résistance encore surveillé", () => {
    // Sommet #2 (130 @ idx17) jamais cassé dans ZIGZAG_UP ⇒ toujours "en résistance". Le trend
    // établi par le dernier événement (BOS haussier) fait qu'une cassure continuerait ⇒ BOS.
    const state = computeTrendState(ZIGZAG_UP, { left: 2, right: 2, displacementMult: 0 });
    expect(state.pending.resistance).toEqual({ level: 130, kind: "BOS" });
  });
});
