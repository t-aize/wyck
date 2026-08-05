import type { GetTrendbarsParams } from "../../ctrader/client.ts";

export interface StructureTimeframe {
  label: string;
  period: GetTrendbarsParams["period"];
  periodMs: number;
  /** Historique total à charger, en ms — assez pour deux swings confirmés avec marge. */
  historyMs: number;
  /** Ne change qu'une fois par jour (D1) : refetché une fois par jour calendaire plutôt qu'à
   * chaque poll de structure — cf. useStructure.ts. */
  dailyOnly?: boolean;
}

/**
 * Longueurs de `swings()` (cf. domain/smc/structure.ts) — mêmes valeurs par défaut que Smart
 * Money Concepts [LuxAlgo], le script SMC le plus copié sur TradingView : `length` par défaut 50
 * pour la structure "swing", 5 (non configurable dans le script d'origine) pour la structure
 * "interne". Fixes sur tous les timeframes, comme dans l'original — LuxAlgo ne scale pas non plus
 * par TF, l'utilisateur ajuste manuellement selon le graphique ouvert. Réglables ici si l'usage
 * réel montre qu'elles rendent mal — pas un scope figé comme SYMBOL.
 */
export const SWING_LENGTH = 50;
export const INTERNAL_LENGTH = 5;

/**
 * `historyMs` dimensionné pour garder la même marge de sécurité qu'avant l'alignement sur
 * `SWING_LENGTH=50` (cf. bug déjà vécu : le swing le plus ancien tombait trop près du bord gauche
 * des données chargées quand la marge était trop juste). Même ratio historyMs/longueur que
 * l'ancien calibrage : 1400h/20 (4H, la TF non-daily la plus lente) → 3500h/50 ; 7000h/28 (D1) →
 * 12500h/50.
 *
 * Un 6H a été tenté puis retiré : cTrader n'a pas cette période nativement (cf. TRENDBAR_PERIODS
 * dans constants.ts), donc reconstruite depuis H1 — mais ça revenait vide en pratique contre le
 * vrai serveur MCP, très probablement parce que "1H" et "6H" redemandaient alors exactement les
 * mêmes bougies H1, en parallèle, dans le même batch (cf. useStructure.ts). Toutes les périodes
 * ci-dessous sont désormais distinctes — plus aucune requête dupliquée dans un même cycle.
 */
const NON_DAILY_HISTORY_MS = 3500 * 60 * 60_000;

export const STRUCTURE_TIMEFRAMES: StructureTimeframe[] = [
  { label: "5M", period: "M_5", periodMs: 5 * 60_000, historyMs: NON_DAILY_HISTORY_MS },
  { label: "15M", period: "M_15", periodMs: 15 * 60_000, historyMs: NON_DAILY_HISTORY_MS },
  { label: "1H", period: "H_1", periodMs: 60 * 60_000, historyMs: NON_DAILY_HISTORY_MS },
  { label: "4H", period: "H_4", periodMs: 4 * 60 * 60_000, historyMs: NON_DAILY_HISTORY_MS },
  {
    label: "D1",
    period: "D_1",
    periodMs: 24 * 60 * 60_000,
    historyMs: 12500 * 60 * 60_000, // ~521 jours — SWING_LENGTH=50 a besoin de bien plus que les 146 jours des autres TF
    dailyOnly: true,
  },
];
