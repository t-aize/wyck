/**
 * Biais global : synthèse d'une phrase unique à partir de tout ce qu'aurum sait déjà (structure
 * multi-timeframe, contexte macro, calendrier). Hiérarchie plutôt que moyenne plate, sur le
 * principe "Triple Screen"/top-down d'analyse multi-timeframe : le timeframe le plus haut fixe la
 * direction et n'est jamais renversé par un timeframe plus bas ; les timeframes bas ne
 * font que confirmer/renforcer une conviction déjà fixée, jamais la direction elle-même ; le
 * contexte macro (COT/dollar large/taux réel) est un modificateur de conviction, borné, jamais un
 * facteur de direction à lui seul (cohérent avec la pratique réelle où on attend toujours une
 * confirmation technique même quand le COT penche déjà dans un sens) ; le calendrier n'est qu'un
 * avertissement, jamais un facteur de direction non plus.
 */

import type { MacroSnapshot } from "../macro.ts";
import { type CalendarEvent, classifyImpact, isGoldRelevant } from "../news.ts";
import type { StructureBias, StructureRow } from "./structure.ts";

/** Fenêtre avant un événement gold à fort impact pour déclencher un avertissement. */
const CAUTION_WINDOW_MS = 90 * 60_000;

export type Conviction = "strong" | "moderate" | "weak";
export type MacroAlignment = "aligned" | "against" | "neutral" | "unavailable";
export type MacroFactorName = "cot" | "usdBroad" | "realYield";

export interface MacroFactor {
  factor: MacroFactorName;
  alignment: MacroAlignment;
  change: number | undefined;
}

export interface ConfirmationDetail {
  label: string;
  biasMatch: boolean;
  signalMatch: boolean;
  sweepMatch: boolean;
  confirms: boolean;
}

export interface CalendarCaution {
  event: CalendarEvent;
  minutesUntil: number;
}

export interface OverallBias {
  /** 1/-1/0 — réutilise StructureBias, 0 recouvre à la fois "neutre" et "pas de biais clair" (données absentes ou ancrage en désaccord). */
  direction: StructureBias;
  anchor: { d1: StructureBias; h4: StructureBias } | undefined;
  /** `undefined` ssi `direction === 0` : pas de direction, pas de conviction à donner dessus. */
  conviction: Conviction | undefined;
  confirmations: ConfirmationDetail[];
  confirmationCount: number;
  macroFactors: MacroFactor[];
  macroAlignedCount: number;
  macroAgainstCount: number;
  /** Indépendant de `direction` : un avertissement calendrier reste pertinent même sans biais clair. */
  caution: CalendarCaution | undefined;
}

function findRow(rows: StructureRow[], label: string): StructureRow | undefined {
  return rows.find((row) => row.label === label);
}

/** D1 et 4H forment l'ancrage — jamais les timeframes plus bas, cf. commentaire en tête de fichier. */
function computeAnchor(
  rows: StructureRow[] | undefined,
): { d1: StructureBias; h4: StructureBias } | undefined {
  if (!rows) return undefined;
  const d1 = findRow(rows, "D1");
  const h4 = findRow(rows, "4H");
  if (!d1 || !h4) return undefined;
  return { d1: d1.snapshot.bias, h4: h4.snapshot.bias };
}

/** Un des deux neutre, ou en désaccord ⇒ pas de direction forcée : `0` (jamais de "biais deviné"). */
function computeDirection(anchor: ReturnType<typeof computeAnchor>): StructureBias {
  if (!anchor) return 0;
  return anchor.d1 === anchor.h4 && anchor.d1 !== 0 ? anchor.d1 : 0;
}

const CONFIRMATION_LABELS = ["1H", "15M", "5M"] as const;

/** Une ligne manquante ne confirme pas — ce n'est pas une erreur, juste une absence de confirmation. */
function computeConfirmations(
  rows: StructureRow[],
  direction: StructureBias,
): ConfirmationDetail[] {
  return CONFIRMATION_LABELS.map((label) => {
    const row = findRow(rows, label);
    if (!row) {
      return { label, biasMatch: false, signalMatch: false, sweepMatch: false, confirms: false };
    }
    const { snapshot } = row;
    const biasMatch = snapshot.bias === direction;
    const signalMatch = snapshot.signalType !== undefined && snapshot.signalDir === direction;
    const sweepMatch = direction === 1 ? snapshot.sweepLow : snapshot.sweepHigh;
    return {
      label,
      biasMatch,
      signalMatch,
      sweepMatch,
      confirms: biasMatch || signalMatch || sweepMatch,
    };
  });
}

/**
 * Sur la tendance (`change`), jamais le niveau brut — un COT déjà extrême ou un taux déjà haut
 * n'est pas en soi "pour" ou "contre" une direction, seule son évolution récente l'est. COT évolue
 * dans le même sens que `direction` pour être aligné (positionnement qui va dans le sens) ; dollar
 * large et taux réel évoluent en sens inverse pour être alignés (dollar/taux qui baissent =
 * haussier pour l'or — corrélations déjà documentées dans macro.ts).
 */
function computeMacroFactors(
  macro: MacroSnapshot | undefined,
  direction: StructureBias,
): MacroFactor[] {
  function alignment(change: number | undefined, invert: boolean): MacroAlignment {
    if (change === undefined) return "unavailable";
    if (change === 0) return "neutral";
    const sameSign = Math.sign(change) === direction;
    return (invert ? !sameSign : sameSign) ? "aligned" : "against";
  }

  return [
    { factor: "cot", alignment: alignment(macro?.cot?.change, false), change: macro?.cot?.change },
    {
      factor: "usdBroad",
      alignment: alignment(macro?.usdBroad?.change, true),
      change: macro?.usdBroad?.change,
    },
    {
      factor: "realYield",
      alignment: alignment(macro?.realYield?.change, true),
      change: macro?.realYield?.change,
    },
  ];
}

const CONVICTION_BASE_BY_CONFIRMATION_COUNT: Conviction[] = [
  "weak",
  "moderate",
  "strong",
  "strong",
];
const CONVICTION_LEVELS: Conviction[] = ["weak", "moderate", "strong"];

/** Confirmation technique = base ; macro = modificateur borné à ±1 cran, jamais plus (le macro seul ne peut jamais faire passer 0 confirmation à "strong"). */
function computeConviction(
  confirmationCount: number,
  macroAlignedCount: number,
  macroAgainstCount: number,
): Conviction {
  const base = CONVICTION_BASE_BY_CONFIRMATION_COUNT[confirmationCount] ?? "weak";
  const delta = macroAlignedCount >= 2 ? 1 : macroAgainstCount >= 2 ? -1 : 0;
  const index = CONVICTION_LEVELS.indexOf(base);
  const bounded = Math.min(CONVICTION_LEVELS.length - 1, Math.max(0, index + delta));
  return CONVICTION_LEVELS[bounded]!;
}

/** Même filtre que NewsPanel (or-pertinent + fort impact), pour rester cohérent avec ce qui s'affiche déjà dans le calendrier. */
function computeCaution(calendar: CalendarEvent[], now: Date): CalendarCaution | undefined {
  const nowMs = now.getTime();
  const soonest = calendar
    .filter((event) => isGoldRelevant(event) && classifyImpact(event.impact) === "high")
    .filter((event) => event.timestamp >= nowMs && event.timestamp - nowMs <= CAUTION_WINDOW_MS)
    .sort((a, b) => a.timestamp - b.timestamp)[0];

  if (!soonest) return undefined;
  return { event: soonest, minutesUntil: Math.round((soonest.timestamp - nowMs) / 60_000) };
}

export function computeOverallBias(
  structureRows: StructureRow[] | undefined,
  macro: MacroSnapshot | undefined,
  calendar: CalendarEvent[],
  now: Date,
): OverallBias {
  const anchor = computeAnchor(structureRows);
  const direction = computeDirection(anchor);
  const rows = structureRows ?? [];

  const confirmations = direction !== 0 ? computeConfirmations(rows, direction) : [];
  const confirmationCount = confirmations.filter((c) => c.confirms).length;

  const macroFactors = direction !== 0 ? computeMacroFactors(macro, direction) : [];
  const macroAlignedCount = macroFactors.filter((f) => f.alignment === "aligned").length;
  const macroAgainstCount = macroFactors.filter((f) => f.alignment === "against").length;

  const conviction =
    direction !== 0
      ? computeConviction(confirmationCount, macroAlignedCount, macroAgainstCount)
      : undefined;

  return {
    direction,
    anchor,
    conviction,
    confirmations,
    confirmationCount,
    macroFactors,
    macroAlignedCount,
    macroAgainstCount,
    caution: computeCaution(calendar, now),
  };
}
