/**
 * Parse des figures ForexFactory (`forecast` / `previous`).
 *
 * Le flux envoie du texte libre, jamais un nombre : `"255K"`, `"0.3%"`,
 * `"-1.2M"`, `"2.48T"`. Sans ce parse, on ne peut pas comparer forecast vs
 * previous.
 */

const FIGURE = /^(-?[\d.,]+)\s*([KMBT])?%?$/i;

const MULTIPLIER: Record<string, number> = {
  K: 1e3,
  M: 1e6,
  B: 1e9,
  T: 1e12,
};

/**
 * Convertit une figure texte en nombre.
 *
 * - `"3.5%"` → `3.5` (le `%` est ignoré : on compare des grandeurs, pas des unités)
 * - `"255K"` → `255000`
 * - `"-1.2M"` → `-1200000`
 * - `"2.48T"` → `2.48e12`
 * - `"3.26|1.1"` / `""` / non numérique → `undefined` (pas de pari en aval)
 *
 * @example
 * parseFigure("255K") // 255000
 * parseFigure("0.3%") // 0.3
 * parseFigure("2.48T") // 2480000000000
 * parseFigure("")     // undefined
 */
export function parseFigure(raw: string): number | undefined {
  const match = raw?.trim().match(FIGURE);
  if (!match) return undefined;
  const num = Number(match[1]!.replace(/,/g, ""));
  if (Number.isNaN(num)) return undefined;
  const suffix = match[2]?.toUpperCase();
  return num * (suffix ? (MULTIPLIER[suffix] ?? 1) : 1);
}
