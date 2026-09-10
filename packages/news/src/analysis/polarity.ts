/**
 * Nature d'un indicateur macro — **avant** de le lire pour un instrument.
 *
 * Ce fichier ne sait pas ce qu'est l'or ou l'EURUSD. Il répond seulement :
 * « ce titre, c'est de la croissance, de l'inflation, ou du slack du marché
 * du travail ? » et « forecast vs previous, c'est hawkish ou dovish pour
 * **la devise** de l'event ? ».
 *
 * On compare forecast vs previous, **pas** actual vs forecast : le flux
 * ForexFactory ne renseigne jamais `actual`, même rétroactivement.
 */

/**
 * Famille d'indicateur.
 *
 * - `growth` — activité réelle (NFP, GDP, PMI, retail…). Plus haut = économie
 *   plus forte / banque centrale plus hawkish.
 * - `inflation` — CPI / PCE / PPI. Plus haut = plus hawkish (taux réels).
 * - `labor_slack` — chômage / inscriptions. Plus haut = marché du travail
 *   plus faible / plus dovish.
 */
export type IndicatorKind = "growth" | "inflation" | "labor_slack";

/**
 * Impulsion **de devise** (pas encore de l'instrument).
 * Hawkish = la devise de l'event a tendance à se renforcer.
 */
export type MacroImpulse = "hawkish" | "dovish" | "neutral";

interface PolarityRule {
  pattern: RegExp;
  kind: IndicatorKind;
}

/**
 * Uniquement les indicateurs avec un appel directionnel étayé. Trade Balance,
 * discours FOMC, etc. en sont absents **volontairement** : pas de badge plutôt
 * qu'un pari. Ils n'ont d'ailleurs souvent ni forecast ni previous dans le flux.
 */
const POLARITY_TABLE: PolarityRule[] = [
  { pattern: /non-?farm|\bnfp\b|\badp\b/i, kind: "growth" },
  { pattern: /\bgdp\b(?!.*price)/i, kind: "growth" },
  { pattern: /\bpmi\b/i, kind: "growth" },
  { pattern: /retail sales/i, kind: "growth" },
  { pattern: /average hourly earnings/i, kind: "growth" },
  { pattern: /\bjolts\b/i, kind: "growth" },
  { pattern: /consumer (confidence|sentiment)/i, kind: "growth" },
  { pattern: /building permits|housing starts|existing home sales/i, kind: "growth" },
  { pattern: /\b(cpi|pce|ppi)\b/i, kind: "inflation" },
  { pattern: /unemployment (rate|claims)/i, kind: "labor_slack" },
  { pattern: /jobless claims|claimant count/i, kind: "labor_slack" },
];

/**
 * Famille de l'indicateur, ou `undefined` si le titre n'est pas dans la table
 * (pas de pari — {@link instrumentBias} renverra `undefined`).
 */
export function indicatorKind(title: string): IndicatorKind | undefined {
  return POLARITY_TABLE.find((rule) => rule.pattern.test(title))?.kind;
}

/**
 * Forecast vs previous → impulsion de devise.
 *
 * Pour `labor_slack`, la lecture est inversée : un chômage **au-dessus** du
 * previous est dovish, pas hawkish.
 *
 * @example
 * macroImpulse("growth", 255_000, 230_000)      // "hawkish"
 * macroImpulse("labor_slack", 230_000, 215_000) // "dovish"
 */
export function macroImpulse(
  kind: IndicatorKind,
  forecast: number,
  previous: number,
): MacroImpulse {
  if (forecast === previous) return "neutral";
  const readingUp = forecast > previous;
  if (kind === "labor_slack") return readingUp ? "dovish" : "hawkish";
  return readingUp ? "hawkish" : "dovish";
}
