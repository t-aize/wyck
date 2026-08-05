/**
 * Biais global : synthèse d'une phrase unique à partir de tout ce qu'aurum sait déjà (structure
 * multi-timeframe, calendrier). Hiérarchie plutôt que moyenne plate, sur le principe "Triple
 * Screen"/top-down d'analyse multi-timeframe : le timeframe le plus haut fixe la direction et
 * n'est jamais renversé par un timeframe plus bas ; les timeframes bas ne font que
 * confirmer/renforcer une conviction déjà fixée, jamais la direction elle-même ; le calendrier
 * n'est qu'un avertissement, jamais un facteur de direction non plus.
 */

import { type CalendarEvent, classifyImpact, isGoldRelevant } from "../news.ts";
import type { StructureBias, StructureRow } from "./structure.ts";

/** Fenêtre avant un événement gold à fort impact pour déclencher un avertissement. */
const CAUTION_WINDOW_MS = 90 * 60_000;

export type Conviction = "strong" | "moderate" | "weak";

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
  /**
   * `true` seulement quand D1 et 4H sont tous les deux directionnels et s'opposent (un haussier,
   * l'autre baissier) — une vraie tension structurelle, distincte du cas banal où l'un des deux
   * est simplement encore neutre (pas assez de swing formé). Les deux cas donnent `direction: 0`,
   * mais seul le premier mérite d'être mis en avant plutôt que traité comme "pas encore de
   * signal".
   */
  anchorConflict: boolean;
  /** `undefined` ssi `direction === 0` : pas de direction, pas de conviction à donner dessus. */
  conviction: Conviction | undefined;
  confirmations: ConfirmationDetail[];
  confirmationCount: number;
  /** Indépendant de `direction` : un avertissement calendrier reste pertinent même sans biais clair. */
  caution: CalendarCaution | undefined;
  /**
   * Repli sur 1H (avec 15M/5M en confirmation) quand D1 ET 4H sont tous les deux neutres — pas un
   * conflit, juste rien de formé sur les plus hauts TF. Volontairement absent des autres cas
   * (conflit, un seul neutre) : ce n'est pas un filet de sécurité général, juste de quoi trader
   * quand même sur une lecture plus courte quand les deux plus hauts TF ne disent rien du tout.
   * `undefined` aussi si 1H lui-même est neutre — il n'y a alors vraiment rien à proposer.
   */
  secondary: SecondaryBias | undefined;
}

export interface SecondaryBias {
  /** Biais du 1H lui-même — jamais 0 quand ce champ existe (cf. commentaire sur `secondary`). */
  direction: StructureBias;
  confirmations: ConfirmationDetail[];
  confirmationCount: number;
  conviction: Conviction;
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

/** Distingue le désaccord réel (les deux directionnels, opposés) du cas banal (un des deux encore neutre). */
function computeAnchorConflict(anchor: ReturnType<typeof computeAnchor>): boolean {
  if (!anchor) return false;
  return anchor.d1 !== 0 && anchor.h4 !== 0 && anchor.d1 !== anchor.h4;
}

const PRIMARY_CONFIRMATION_LABELS = ["1H", "15M", "5M"] as const;
const SECONDARY_CONFIRMATION_LABELS = ["15M", "5M"] as const;

/** Une ligne manquante ne confirme pas — ce n'est pas une erreur, juste une absence de confirmation. */
function computeConfirmations(
  rows: StructureRow[],
  direction: StructureBias,
  labels: readonly string[],
): ConfirmationDetail[] {
  return labels.map((label) => {
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

const CONVICTION_BASE_BY_CONFIRMATION_COUNT: Conviction[] = [
  "weak",
  "moderate",
  "strong",
  "strong",
];

/** Uniquement basé sur le nombre de confirmations MTF — plus de modificateur macro. */
function computeConviction(confirmationCount: number): Conviction {
  return CONVICTION_BASE_BY_CONFIRMATION_COUNT[confirmationCount] ?? "weak";
}

/** Repli 1H : cf. commentaire sur `OverallBias.secondary`. */
function computeSecondary(
  rows: StructureRow[],
  anchor: ReturnType<typeof computeAnchor>,
): SecondaryBias | undefined {
  if (anchor?.d1 !== 0 || anchor.h4 !== 0) return undefined;

  const direction = findRow(rows, "1H")?.snapshot.bias;
  if (!direction) return undefined;

  const confirmations = computeConfirmations(rows, direction, SECONDARY_CONFIRMATION_LABELS);
  const confirmationCount = confirmations.filter((c) => c.confirms).length;

  return {
    direction,
    confirmations,
    confirmationCount,
    conviction: computeConviction(confirmationCount),
  };
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
  calendar: CalendarEvent[],
  now: Date,
): OverallBias {
  const anchor = computeAnchor(structureRows);
  const direction = computeDirection(anchor);
  const anchorConflict = computeAnchorConflict(anchor);
  const rows = structureRows ?? [];

  const confirmations =
    direction !== 0 ? computeConfirmations(rows, direction, PRIMARY_CONFIRMATION_LABELS) : [];
  const confirmationCount = confirmations.filter((c) => c.confirms).length;

  const conviction = direction !== 0 ? computeConviction(confirmationCount) : undefined;

  return {
    direction,
    anchor,
    anchorConflict,
    conviction,
    confirmations,
    confirmationCount,
    caution: computeCaution(calendar, now),
    secondary: computeSecondary(rows, anchor),
  };
}
