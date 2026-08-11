import type { GetTrendbarsParams } from "../../ctrader/client.ts";

/** Les 3 timeframes suivis par le panneau structure — aussi la liste fermée proposée pour
 * `atr timeframe` (cf. domain/commands.ts) : le mode ATR ne peut choisir que parmi des bougies déjà
 * fetchées ici, pas un TF arbitraire qui demanderait un appel réseau dédié. */
export const ATR_TIMEFRAME_LABELS = ["M5", "M15", "H1"] as const;
export type AtrTimeframeLabel = (typeof ATR_TIMEFRAME_LABELS)[number];

export interface TrendTimeframe {
  label: AtrTimeframeLabel;
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
 * compte réel est toujours inférieur au compte "attendu" pour une plage donnée.
 *
 * Valeur H1 (3500h) : celle qui avait déjà réglé ce problème avant la suppression d'une version
 * antérieure de ce module — appliquée telle quelle plutôt que réestimée à l'aveugle. M5/M15 : pas
 * la même valeur que H1 (145 jours de bougies 5 min n'apporte rien pour une lecture de structure
 * court terme, et ne fait qu'ajouter des fenêtres de requête pour rien une fois `requestCapMs`
 * corrigé côté useTrend.ts) — quelques semaines, proportionnellement à la finesse de la bougie,
 * largement suffisant pour observer plusieurs cycles HH/HL avec la fractale swing de chaque TF.
 */
const HISTORY_MS_M5 = 10 * 24 * 60 * 60_000; // 10 jours
const HISTORY_MS_M15 = 20 * 24 * 60 * 60_000; // 20 jours
const HISTORY_MS_H1 = 3500 * 60 * 60_000; // ~146 jours

/**
 * Paramètres de départ par TF — à ajuster en usage réel/backtest, ce ne sont pas des vérités (cf.
 * le tableau de référence fourni : lookback interne/swing/mult. displacement par TF, milieu de
 * fourchette retenu quand une plage était donnée). `minSwingPct` n'a pas de valeur de référence
 * donnée — départ conservateur, plus resserré sur les TF les plus bruités (M5/M15) que sur H1 dont
 * le lookback déjà large filtre davantage de bruit nativement. H1 garde tout de même un plancher
 * minime (0.03%) plutôt que 0% strict, en garde-fou contre un pic de données aberrant (spread
 * anormal, gap de rollover) plutôt que comme véritable filtre de bruit.
 */
function timeframe(
  label: AtrTimeframeLabel,
  period: GetTrendbarsParams["period"],
  periodMs: number,
  params: Pick<
    TrendTimeframe,
    "historyMs" | "internalLength" | "swingLength" | "minSwingPct" | "displacementMult"
  >,
): TrendTimeframe {
  return { label, period, periodMs, ...params };
}

export const TREND_TIMEFRAMES: TrendTimeframe[] = [
  timeframe("M5", "M_5", 5 * 60_000, {
    historyMs: HISTORY_MS_M5,
    internalLength: 3,
    swingLength: 18,
    minSwingPct: 0.1,
    displacementMult: 1.5,
  }),
  timeframe("M15", "M_15", 15 * 60_000, {
    historyMs: HISTORY_MS_M15,
    internalLength: 5,
    swingLength: 32,
    minSwingPct: 0.05,
    displacementMult: 1.3,
  }),
  timeframe("H1", "H_1", 60 * 60_000, {
    historyMs: HISTORY_MS_H1,
    internalLength: 6,
    swingLength: 30,
    minSwingPct: 0.03,
    displacementMult: 1.2,
  }),
];
