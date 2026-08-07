import type { GetTrendbarsParams } from "../../ctrader/client.ts";

export interface TrendTimeframe {
  label: string;
  period: GetTrendbarsParams["period"];
  periodMs: number;
  /** Historique total à charger, en ms. */
  historyMs: number;
  /** Fractale "interne" (mineure) — plus courte, sert au timing d'entrée. */
  internalLength: number;
  /** Fractale "swing" (majeure) — plus longue, définit le biais directionnel principal (colonnes
   * STRUCT/CONFIRME du panneau). */
  swingLength: number;
  /** Filtre les swings trop proches du précédent (bruit), en % du prix — cf. classifyStructuralTrend. */
  minSwingPct: number;
  /** Multiplicateur ATR pour le filtre de displacement — cf. detectStructureEvents. */
  displacementMult: number;
}

/**
 * Historique demandé par TF, en ms. `get_trendbars` ne renvoie que les bougies réellement tradées
 * sur la plage demandée (marché fermé le week-end, éventuelle limite serveur par appel) — le
 * compte réel est toujours inférieur au compte "attendu" pour une plage donnée. Cette valeur
 * (3500h) est celle qui avait déjà réglé ce problème avant la suppression d'une version antérieure
 * de ce module — appliquée telle quelle plutôt que réestimée à l'aveugle.
 */
const HISTORY_MS = 3500 * 60 * 60_000;

/**
 * Paramètres de départ par TF — à ajuster en usage réel/backtest, ce ne sont pas des vérités (cf.
 * le tableau de référence fourni : lookback interne/swing/mult. displacement par TF, milieu de
 * fourchette retenu quand une plage était donnée). `minSwingPct` n'a pas de valeur de référence
 * donnée — départ conservateur, plus resserré sur les TF les plus bruités (M5/M15) que sur H1 dont
 * le lookback déjà large filtre davantage de bruit nativement.
 */
function timeframe(
  label: string,
  period: GetTrendbarsParams["period"],
  periodMs: number,
  params: Pick<
    TrendTimeframe,
    "internalLength" | "swingLength" | "minSwingPct" | "displacementMult"
  >,
): TrendTimeframe {
  return { label, period, periodMs, historyMs: HISTORY_MS, ...params };
}

export const TREND_TIMEFRAMES: TrendTimeframe[] = [
  timeframe("M5", "M_5", 5 * 60_000, {
    internalLength: 3,
    swingLength: 18,
    minSwingPct: 0.1,
    displacementMult: 1.5,
  }),
  timeframe("M15", "M_15", 15 * 60_000, {
    internalLength: 5,
    swingLength: 32,
    minSwingPct: 0.05,
    displacementMult: 1.3,
  }),
  timeframe("H1", "H_1", 60 * 60_000, {
    internalLength: 6,
    swingLength: 30,
    minSwingPct: 0,
    displacementMult: 1.2,
  }),
];
