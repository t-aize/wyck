import { describe, expect, test } from "bun:test";
import type { CtraderTrendbar } from "../../ctrader/client.ts";
import {
  classifyEventTrend,
  detectStructureEvents,
  type StructureEvent,
} from "./structureEvents.ts";

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
