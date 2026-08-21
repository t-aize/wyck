import { describe, expect, test } from "bun:test";
import type { CtraderTrendbar } from "../../ctrader/schemas.ts";
import { classifyStructuralTrend } from "./structuralTrend.ts";

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
