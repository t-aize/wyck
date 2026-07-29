import type { GetTrendbarsParams } from "../../ctrader/client.ts";

export interface StructureTimeframe {
  label: string;
  period: GetTrendbarsParams["period"];
  length: number;
  periodMs: number;
  /** Historique total à charger, en ms — assez pour deux pivots confirmés avec marge. */
  historyMs: number;
  /** Ne change qu'une fois par jour (D1) : refetché une fois par jour calendaire plutôt qu'à
   * chaque poll de structure — cf. useStructure.ts. */
  dailyOnly?: boolean;
}

/**
 * 15M=7 / 1H=14 / 4H=20 sont les longueurs par défaut du script d'origine (Pine). 5M et D1 n'ont
 * pas d'équivalent documenté dans le script — recherché sans succès une source faisant autorité
 * (même LuxAlgo, la référence SMC la plus connue, utilise une longueur fixe indépendante du
 * timeframe : ça ne se scale pas dans l'industrie). Les valeurs ci-dessous suivent la régression
 * qui ressort des 3 valeurs d'origine elles-mêmes : `length ≈ -5.7 + 3.25 × log2(minutes_par_
 * bougie)`, un ajustement quasi parfait sur 15M/1H/4H (erreur < 4% sur le point du milieu).
 * Extrapolée sur D1 → 28. Pour 5M, la droite donne 2, bien trop bruyant en pratique sur XAUUSD
 * (quasi chaque mèche deviendrait un pivot) — écart assumé de la régression, length=8 choisi à
 * la place (cohérent avec le 7 du 15M). Des longueurs ajustables ici si l'usage réel montre
 * qu'elles rendent mal — pas un scope figé comme SYMBOL.
 *
 * Un 6H a été tenté puis retiré : cTrader n'a pas cette période nativement (cf. TRENDBAR_PERIODS
 * dans constants.ts), donc reconstruite depuis H1 — mais ça revenait vide en pratique contre le
 * vrai serveur MCP, très probablement parce que "1H" et "6H" redemandaient alors exactement les
 * mêmes bougies H1, en parallèle, dans le même batch (cf. useStructure.ts). Toutes les périodes
 * ci-dessous sont désormais distinctes — plus aucune requête dupliquée dans un même cycle.
 */
export const STRUCTURE_TIMEFRAMES: StructureTimeframe[] = [
  { label: "5M", period: "M_5", length: 8, periodMs: 5 * 60_000, historyMs: 1400 * 60 * 60_000 },
  { label: "15M", period: "M_15", length: 7, periodMs: 15 * 60_000, historyMs: 1400 * 60 * 60_000 },
  { label: "1H", period: "H_1", length: 14, periodMs: 60 * 60_000, historyMs: 1400 * 60 * 60_000 },
  {
    label: "4H",
    period: "H_4",
    length: 20,
    periodMs: 4 * 60 * 60_000,
    historyMs: 1400 * 60 * 60_000,
  },
  {
    label: "D1",
    period: "D_1",
    length: 28,
    periodMs: 24 * 60 * 60_000,
    historyMs: 7000 * 60 * 60_000, // ~292 jours — length=28 a besoin de bien plus que les 58 jours des autres TF
    dailyOnly: true,
  },
];
