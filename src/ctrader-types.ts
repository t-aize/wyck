/**
 * Constantes partagées entre env.ts et ctrader-client.ts.
 *
 * Fichier séparé volontairement : env.ts et ctrader-client.ts s'importent déjà
 * mutuellement au niveau logique (ctrader-client.ts lit `env`), donc s'ils
 * s'importaient aussi l'un l'autre pour ces constantes on aurait un cycle,
 * qui plante au démarrage selon l'ordre d'import (vérifié en pratique). En
 * dépendant tous les deux de ce fichier neutre, ni l'un ni l'autre n'a besoin
 * d'importer son propre importeur.
 */

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
