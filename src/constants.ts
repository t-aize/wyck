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

/** Prix cTrader spot/trendbar : entier à l'échelle x10^5 (ex: 410177000 → 4101.77). */
export const PRICE_SCALE = 100_000;
