/**
 * Constantes partagées entre env.ts et ctrader/client.ts.
 *
 * Fichier séparé volontairement : env.ts et ctrader/client.ts s'importent déjà
 * mutuellement au niveau logique (ctrader/client.ts lit `env`), donc s'ils
 * s'importaient aussi l'un l'autre pour ces constantes on aurait un cycle,
 * qui plante au démarrage selon l'ordre d'import (vérifié en pratique). En
 * dépendant tous les deux de ce fichier neutre, ni l'un ni l'autre n'a besoin
 * d'importer son propre importeur.
 */

/** Seul symbole tradé par ce panel — pas un réglage, un choix de scope du projet. */
export const SYMBOL = "XAUUSD";

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
