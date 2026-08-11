/**
 * MÉTHODE 2 — événementielle (BOS/CHoCH) : plus réactive mais plus sensible au bruit et aux pièges
 * à liquidité (Judas swing). Règle de confirmation : un CHoCH seul est un signal de retournement
 * *potentiel*, pas une confirmation — la tendance n'est considérée retournée qu'une fois qu'un BOS
 * confirme dans le même sens que le CHoCH précédent. Extrait de trend.ts.
 */

import type { CtraderTrendbar } from "../../ctrader/client.ts";
import { computeAtrSeries, hasDisplacement } from "./atr.ts";
import { detectSwings, type FractalOptions } from "./swings.ts";
import type { Trend } from "./types.ts";

export type StructureEventKind = "BOS" | "CHoCH";

export interface StructureEvent {
  index: number;
  price: number;
  kind: StructureEventKind;
  direction: 1 | -1;
  /** Métadonnée informative — n'altère pas rawEvent/confirmedEvent (comme la référence), sert à
   * juger la confiance du signal côté appelant. */
  displacementOk: boolean;
}

export interface StructureDetectionOptions extends FractalOptions {
  atrPeriod?: number;
  displacementMult?: number;
}

/** Niveau encore surveillé (pas encore cassé) et ce que sa cassure produirait, compte tenu de la
 * tendance actuelle — BOS si ça continue le trend en cours à cette échelle, CHoCH si ça l'inverse. */
export interface PendingLevel {
  level: number;
  kind: StructureEventKind;
}

export interface PendingLevels {
  /** Prochaine résistance (niveau haut) — cassure à la hausse. `undefined` si aucun swing haut non
   * cassé n'a encore reformé depuis le dernier événement. */
  resistance: PendingLevel | undefined;
  /** Symétrique côté support (niveau bas, cassure à la baisse). */
  support: PendingLevel | undefined;
}

export interface StructureDetectionResult {
  events: StructureEvent[];
  pending: PendingLevels;
}

/** Retire de `open` (en place) toute entrée cassée en clôture par cette bougie — jamais celle
 * formée à cette bougie même (une mèche ne peut pas casser son propre swing, cf. `index !== i`). */
function pruneBroken(
  open: { index: number; price: number }[],
  i: number,
  close: number,
  side: "high" | "low",
): void {
  for (let k = open.length - 1; k >= 0; k--) {
    const broken = side === "high" ? close > open[k]!.price : close < open[k]!.price;
    if (broken && open[k]!.index !== i) open.splice(k, 1);
  }
}

/**
 * BOS = cassure (sur clôture de corps, jamais une mèche) du dernier swing DANS le sens de la
 * tendance active. CHoCH = cassure de ce même swing dans le sens OPPOSÉ. `elif` volontaire (une
 * bougie ne peut produire qu'un seul événement) et garde `lastHigh.index !== i` (le swing qui
 * vient de se former à cette bougie ne peut pas être "cassé" par elle-même) — même logique que la
 * référence, pas une simplification.
 *
 * `pending` expose les niveaux encore surveillés à la toute fin de l'historique (résistance/support
 * "prochains BOS/CHoCH"), mais PAS en réutilisant lastHigh/lastLow ci-dessus : ceux-ci ne retiennent
 * que le DERNIER swing détecté, pas forcément le plus proche du prix. Un swing plus ancien peut
 * rester valide (jamais cassé en clôture) alors qu'un swing plus récent — souvent une simple mèche,
 * cf. `detectLatestSweep` — l'a remplacé comme "dernier formé" sans jamais casser l'ancien lui-même
 * (le swing se détecte sur high/low, la cassure se vérifie sur close : les deux peuvent diverger).
 * `openHighs`/`openLows` gardent donc TOUS les swings encore non cassés en clôture, et le niveau
 * affiché est le plus proche du prix parmi eux (min pour une résistance, max pour un support — les
 * deux sont forcément du bon côté du prix, cf. `closerOf` dans TrendPanel.tsx qui applique le même
 * principe entre fractales swing/interne).
 */
export function detectStructureEvents(
  bars: CtraderTrendbar[],
  { left, right, atrPeriod = 14, displacementMult = 1.2 }: StructureDetectionOptions,
): StructureDetectionResult {
  const { highs, lows } = detectSwings(bars, { left, right });
  const atr = computeAtrSeries(bars, atrPeriod);

  let trend: Trend = 0;
  let lastHigh: { index: number; price: number } | undefined;
  let lastLow: { index: number; price: number } | undefined;
  const openHighs: { index: number; price: number }[] = [];
  const openLows: { index: number; price: number }[] = [];
  const events: StructureEvent[] = [];

  for (let i = 0; i < bars.length; i++) {
    const close = bars[i]!.close;
    if (highs[i] !== undefined) {
      lastHigh = { index: i, price: highs[i]! };
      openHighs.push(lastHigh);
    }
    if (lows[i] !== undefined) {
      lastLow = { index: i, price: lows[i]! };
      openLows.push(lastLow);
    }
    pruneBroken(openHighs, i, close, "high");
    pruneBroken(openLows, i, close, "low");

    if (lastHigh && close > lastHigh.price && lastHigh.index !== i) {
      events.push({
        index: i,
        price: lastHigh.price,
        kind: trend === -1 ? "CHoCH" : "BOS",
        direction: 1,
        displacementOk: hasDisplacement(bars, i, atr, displacementMult),
      });
      trend = 1;
      lastHigh = undefined;
    } else if (lastLow && close < lastLow.price && lastLow.index !== i) {
      events.push({
        index: i,
        price: lastLow.price,
        kind: trend === 1 ? "CHoCH" : "BOS",
        direction: -1,
        displacementOk: hasDisplacement(bars, i, atr, displacementMult),
      });
      trend = -1;
      lastLow = undefined;
    }
  }

  const closestHigh = openHighs.reduce<number | undefined>(
    (min, h) => (min === undefined || h.price < min ? h.price : min),
    undefined,
  );
  const closestLow = openLows.reduce<number | undefined>(
    (max, l) => (max === undefined || l.price > max ? l.price : max),
    undefined,
  );

  return {
    events,
    pending: {
      resistance:
        closestHigh !== undefined
          ? { level: closestHigh, kind: trend === -1 ? "CHoCH" : "BOS" }
          : undefined,
      support:
        closestLow !== undefined
          ? { level: closestLow, kind: trend === 1 ? "CHoCH" : "BOS" }
          : undefined,
    },
  };
}

export interface EventTrendResult {
  raw: Trend;
  confirmed: Trend;
  /** Displacement du dernier événement (raw) — undefined si aucun événement dans l'historique. */
  lastDisplacementOk: boolean | undefined;
}

/** Rejoue la liste d'événements pour n'en garder que l'état final (raw/confirmed) — équivalent à
 * lire `.iloc[-1]` des deux séries pandas de la référence, qu'on ne matérialise pas ici puisque
 * seul l'état courant intéresse le panneau. */
export function classifyEventTrend(events: StructureEvent[]): EventTrendResult {
  let raw: Trend = 0;
  let confirmed: Trend = 0;
  let pendingChochDir: -1 | 1 | undefined;
  let lastDisplacementOk: boolean | undefined;

  for (const e of events) {
    raw = e.direction;
    lastDisplacementOk = e.displacementOk;

    if (e.kind === "CHoCH") {
      pendingChochDir = e.direction;
    } else if (pendingChochDir === e.direction) {
      confirmed = e.direction;
      pendingChochDir = undefined;
    } else if (confirmed === 0) {
      confirmed = e.direction;
    }
  }

  return { raw, confirmed, lastDisplacementOk };
}
