/**
 * Conversions prix/volume. lotSize et digits viennent de l'instrument (cf. instrument/specs.ts) :
 * plus de constantes XAUUSD en dur.
 *
 * Volume API = lots × lotSize × 100.
 */

/** Arrondit un prix affiché à la précision acceptée par l'API pour ce symbole. */
export function roundPrice(price: number, digits: number): number {
  const factor = 10 ** digits;
  return Math.round(price * factor) / factor;
}

export function toLots(volume: number, lotSize: number): number {
  return volume / (lotSize * 100);
}

/** Inverse de toLots : lots → volume API. Arrondit uniquement l'imprécision flottante. */
export function toVolume(lots: number, lotSize: number): number {
  return Math.round(lots * lotSize * 100);
}

/** Distance de prix (déjà affiché, pas x10^5) convertie en pips. */
export function toPips(priceDistance: number, pipSize: number): number {
  if (pipSize <= 0) return 0;
  return Math.round(Math.abs(priceDistance) / pipSize);
}
