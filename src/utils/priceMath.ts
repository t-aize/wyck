/**
 * Fonctions de conversion/arrondi dérivées des constantes de prix/volume (cf. constants.ts) —
 * séparées des valeurs elles-mêmes pour garder constants.ts en pur `export const`, sans logique.
 */

import { LOT_VOLUME } from "../constants.ts";

/** XAUUSD n'accepte que 2 décimales de prix côté API ("more digits than symbol allows"). */
const PRICE_DIGITS = 2;

/** Arrondit un prix affiché à la précision acceptée par l'API pour ce symbole. */
export function roundPrice(price: number): number {
  const factor = 10 ** PRICE_DIGITS;
  return Math.round(price * factor) / factor;
}

/** Convertit un volume API (1/100 d'once) en lots — approximation valable pour XAUUSD uniquement. */
export function toLots(volume: number): number {
  return volume / LOT_VOLUME;
}

/** Inverse de toLots : lots → volume API (1/100 d'once). Arrondit uniquement l'imprécision
 * flottante de la multiplication — pas de snap sur VOLUME_STEP ici (contrairement à
 * trading/risk.ts#computeVolume) : cette fonction inverse un volume déjà valide venu de l'API,
 * elle n'en dérive pas un nouveau depuis du calcul de risque. */
export function toVolume(lots: number): number {
  return Math.round(lots * LOT_VOLUME);
}

// ponytail: convention standard XAUUSD (1 pip = 0.10$) — à ajuster si le broker en utilise une autre.
const PIP_SIZE = 0.1;

/** Distance de prix (déjà affiché, pas x10^5) convertie en pips. */
export function toPips(priceDistance: number): number {
  return Math.round(Math.abs(priceDistance) / PIP_SIZE);
}
