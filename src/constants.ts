/**
 * Constantes partagées entre plusieurs modules qui, sinon, s'importeraient
 * mutuellement (cycle d'import, plante au démarrage selon l'ordre — vérifié en
 * pratique). En dépendant tous de ce fichier neutre, aucun n'a besoin d'importer
 * son propre importeur. Les fonctions dérivées de ces valeurs (roundPrice, toLots,
 * toPips) vivent dans utils/priceMath.ts — ce fichier ne garde que les valeurs.
 */

import { homedir } from "node:os";
import { join } from "node:path";

/** Seul symbole tradé par ce panel — pas un réglage, un choix de scope du projet. */
export const SYMBOL = "XAUUSD";

/** Dossier de données de l'app dans le homedir (config, cache) — indépendant du dossier de lancement. */
export const APP_DATA_DIR = join(homedir(), ".aurum");

/** Proposée par `settings url` (cf. commands/settings.ts) quand aucun argument n'est fourni — texte
 * affiché dans la ligne de feedback, sélectionnable/copiable via le copier-coller déjà supporté par
 * le terminal (cf. useTerminalShortcuts.ts). */
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

/** Prix cTrader : entier à l'échelle x10^5 (ex: 410177000 → 4101.77). */
export const PRICE_SCALE = 100_000;

/**
 * XAUUSD (métaux) : 1 lot = 100 onces, prix coté en $/once. Volume API = onces × 100
 * (cf. commentaire équivalent dans PositionsPanel.tsx). Seul symbole tradé ici — à
 * revoir si d'autres classes d'actifs sont ajoutées un jour (lotSize/valeur du point
 * diffèrent : forex, indices, crypto).
 */
export const LOT_VOLUME = 10_000; // 1.00 lot en unités API
