/**
 * Constantes partagées entre plusieurs modules qui, sinon, s'importeraient
 * mutuellement (cycle d'import, plante au démarrage selon l'ordre — vérifié en
 * pratique). En dépendant tous de ce fichier neutre, aucun n'a besoin d'importer
 * son propre importeur.
 */

import { homedir } from "node:os";
import { join } from "node:path";

/** Seul symbole tradé par ce panel — pas un réglage, un choix de scope du projet. */
export const SYMBOL = "XAUUSD";

/** Dossier de données de l'app dans le homedir (config, cache) — indépendant du dossier de lancement. */
export const APP_DATA_DIR = join(homedir(), ".aurum");

export const TRENDBAR_PERIODS = [
  "M_1",
  "M_5",
  "M_15",
  "M_30",
  "H_1",
  "H_4",
  "D_1",
  "W_1",
  "MN_1",
] as const;
export type TrendbarPeriod = (typeof TRENDBAR_PERIODS)[number];

/** Prix cTrader : entier à l'échelle x10^5 (ex: 410177000 → 4101.77). */
export const PRICE_SCALE = 100_000;

/** XAUUSD n'accepte que 2 décimales de prix côté API ("more digits than symbol allows"). */
const PRICE_DIGITS = 2;

/** Arrondit un prix affiché à la précision acceptée par l'API pour ce symbole. */
export function roundPrice(price: number): number {
  const factor = 10 ** PRICE_DIGITS;
  return Math.round(price * factor) / factor;
}

/**
 * XAUUSD (métaux) : 1 lot = 100 onces, prix coté en $/once. Volume API = onces × 100
 * (cf. commentaire équivalent dans PositionsPanel.tsx). Seul symbole tradé ici — à
 * revoir si d'autres classes d'actifs sont ajoutées un jour (lotSize/valeur du point
 * diffèrent : forex, indices, crypto).
 */
export const LOT_VOLUME = 10_000; // 1.00 lot en unités API

/** Convertit un volume API (1/100 d'once) en lots — approximation valable pour XAUUSD uniquement. */
export function toLots(volume: number): number {
  return volume / LOT_VOLUME;
}

// ponytail: convention standard XAUUSD (1 pip = 0.10$) — à ajuster si le broker en utilise une autre.
export const PIP_SIZE = 0.1;

/** Distance de prix (déjà affiché, pas x10^5) convertie en pips. */
export function toPips(priceDistance: number): number {
  return Math.round(Math.abs(priceDistance) / PIP_SIZE);
}
