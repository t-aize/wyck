/**
 * Constantes partagées entre plusieurs modules qui, sinon, s'importeraient
 * mutuellement (cycle d'import, plante au démarrage selon l'ordre — vérifié en
 * pratique). En dépendant tous de ce fichier neutre, aucun n'a besoin d'importer
 * son propre importeur. Les fonctions dérivées (roundPrice, toLots, toPips)
 * vivent dans utils/priceMath.ts — ce fichier ne garde que les valeurs.
 */

import { homedir } from "node:os";
import { join } from "node:path";

/** Symbole par défaut au premier lancement — ensuite persisté dans settings.json. */
export const DEFAULT_SYMBOL = "XAUUSD";

/** Dossier de données de l'app dans le homedir (config, cache) — indépendant du dossier de lancement. */
export const APP_DATA_DIR = join(homedir(), ".aurum");

/** Proposée par `settings url` (cf. commands/settings.ts) quand aucun argument n'est fourni. */
export const DEFAULT_MCP_URL = "https://mcp.ctrader.com/trading/mcp";

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

/** Durée d'une bougie par timeframe, pour aligner le refresh auto ATR sur sa vraie clôture.
 * Approximatif pour D_1/W_1/MN_1 : `Math.ceil(now / ms) * ms` suppose des bougies de taille
 * fixe alignées sur epoch 0. */
export const TRENDBAR_PERIOD_MS: Record<TrendbarPeriod, number> = {
  M_1: 60_000,
  M_5: 5 * 60_000,
  M_15: 15 * 60_000,
  M_30: 30 * 60_000,
  H_1: 60 * 60_000,
  H_4: 4 * 60 * 60_000,
  D_1: 24 * 60 * 60_000,
  W_1: 7 * 24 * 60 * 60_000,
  MN_1: 30 * 24 * 60 * 60_000,
};

/** Prix cTrader spot/trendbar : entier à l'échelle x10^5 (ex: 410177000 → 4101.77). */
export const PRICE_SCALE = 100_000;
