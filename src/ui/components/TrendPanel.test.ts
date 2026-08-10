import { describe, expect, test } from "bun:test";
import type { PendingLevel, PendingLevels } from "../../domain/smc/trend.ts";
import { closerResistance, closerSupport } from "./TrendPanel.tsx";

function level(price: number, kind: PendingLevel["kind"] = "BOS"): PendingLevel {
  return { level: price, kind };
}

function pending(level: PendingLevel | undefined): PendingLevels {
  return { resistance: level, support: level };
}

describe("closerResistance", () => {
  test("aucun niveau nulle part ⇒ undefined", () => {
    expect(closerResistance(pending(undefined), pending(undefined))).toBeUndefined();
  });

  test("seul le swing existe ⇒ swing, pas marqué interne", () => {
    const swing = level(4400);
    expect(closerResistance(pending(swing), pending(undefined))).toEqual({
      level: swing,
      fromInternal: false,
    });
  });

  test("seul l'interne existe ⇒ interne, marqué comme tel", () => {
    const internal = level(4360);
    expect(closerResistance(pending(undefined), pending(internal))).toEqual({
      level: internal,
      fromInternal: true,
    });
  });

  test("les deux existent, interne plus proche (prix plus bas) ⇒ interne retenu", () => {
    const swing = level(4400, "BOS");
    const internal = level(4360, "CHoCH");
    expect(closerResistance(pending(swing), pending(internal))).toEqual({
      level: internal,
      fromInternal: true,
    });
  });

  test("les deux existent, swing plus proche (prix plus bas) ⇒ swing retenu", () => {
    const swing = level(4360);
    const internal = level(4400);
    expect(closerResistance(pending(swing), pending(internal))).toEqual({
      level: swing,
      fromInternal: false,
    });
  });

  test("égalité ⇒ le swing gagne (structure majeure par défaut)", () => {
    const swing = level(4400, "BOS");
    const internal = level(4400, "CHoCH");
    expect(closerResistance(pending(swing), pending(internal))).toEqual({
      level: swing,
      fromInternal: false,
    });
  });
});

describe("closerSupport", () => {
  test("les deux existent, interne plus proche (prix plus haut) ⇒ interne retenu", () => {
    const swing = level(4097.65);
    const internal = level(4310.2);
    expect(closerSupport(pending(swing), pending(internal))).toEqual({
      level: internal,
      fromInternal: true,
    });
  });

  test("les deux existent, swing plus proche (prix plus haut) ⇒ swing retenu", () => {
    const swing = level(4310.2);
    const internal = level(4097.65);
    expect(closerSupport(pending(swing), pending(internal))).toEqual({
      level: swing,
      fromInternal: false,
    });
  });

  test("seul le swing existe ⇒ swing, pas marqué interne", () => {
    const swing = level(4097.65);
    expect(closerSupport(pending(swing), pending(undefined))).toEqual({
      level: swing,
      fromInternal: false,
    });
  });
});
