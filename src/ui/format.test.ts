import { describe, expect, test } from "bun:test";
import { sparkline } from "./format.ts";

describe("sparkline", () => {
  test("série vide ⇒ chaîne vide", () => {
    expect(sparkline([])).toBe("");
  });

  test("valeur unique ⇒ niveau médian, pas de division par zéro", () => {
    // 8 niveaux (pas de centre exact) : floor(0.5 × 8) = 4 → "▅".
    expect(sparkline([42])).toBe("▅");
  });

  test("série plate (toutes valeurs égales) ⇒ niveau médian partout", () => {
    expect(sparkline([5, 5, 5, 5])).toBe("▅▅▅▅");
  });

  test("série strictement croissante ⇒ du plus bas au plus haut niveau", () => {
    const result = sparkline([0, 1, 2, 3, 4, 5, 6, 7]);
    expect(result).toBe("▁▂▃▄▅▆▇█");
  });

  test("série strictement décroissante ⇒ symétrique de la croissante", () => {
    const result = sparkline([7, 6, 5, 4, 3, 2, 1, 0]);
    expect(result).toBe("█▇▆▅▄▃▂▁");
  });

  test("longueur du résultat == longueur de l'entrée", () => {
    const values = [4100, 4101, 4099, 4102, 4098.5];
    expect(sparkline(values).length).toBe(values.length);
  });

  test("min et max de la série sont toujours aux niveaux extrêmes", () => {
    const values = [4100, 4110, 4090, 4105, 4095];
    const result = sparkline(values);
    const minIndex = values.indexOf(Math.min(...values));
    const maxIndex = values.indexOf(Math.max(...values));
    expect(result[minIndex]).toBe("▁");
    expect(result[maxIndex]).toBe("█");
  });
});
