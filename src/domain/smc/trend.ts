/**
 * Classification de tendance (Bullish / Bearish / Range) — port fidèle du script Python de
 * référence (fractale à N bougies, PAS le zigzag adaptatif LuxAlgo utilisé dans une version
 * antérieure de ce fichier). Chaque fonction ci-dessous correspond 1:1 à une fonction Python de la
 * référence, même nom de concept, même ordre de conditions (y compris les `elif`/priorités).
 *
 * MÉTHODE 1 — structurelle (`classifyStructuralTrend`, HH/HL vs LH/LL) : la plus fiable,
 * directement issue de la théorie de Dow. Séquence mixte (ex: HH mais LL) = range, pas de biais
 * net — un signal binaire bull/bear génère des faux positifs en range.
 *
 * MÉTHODE 2 — événementielle (`detectStructureEvents` + `classifyEventTrend`, BOS/CHoCH) : plus
 * réactive mais plus sensible au bruit et aux pièges à liquidité (Judas swing). Règle de
 * confirmation : un CHoCH seul est un signal de retournement *potentiel*, pas une confirmation —
 * la tendance n'est considérée retournée qu'une fois qu'un BOS confirme dans le même sens que le
 * CHoCH précédent.
 *
 * Complément : liquidity sweep (mèche au-delà d'un niveau, PAS de clôture au-delà — distinct d'une
 * vraie cassure de structure), filtre de displacement (ATR, écarte les cassures "molles" sans réel
 * order flow — exposé en métadonnée sur l'événement, n'altère pas la classification elle-même,
 * exactement comme la référence), et deux filtres classiques (pas du SMC) : EMA stack + ADX.
 */

import type { CtraderTrendbar } from "../../ctrader/client.ts";

export type Trend = -1 | 0 | 1;

/** ADX > ce seuil ~ marché "en tendance" (seuil usuel, pas une loi physique). */
export const ADX_TRENDING_THRESHOLD = 25;

export interface FractalOptions {
  left: number;
  right: number;
}

interface SwingSeries {
  highs: (number | undefined)[];
  lows: (number | undefined)[];
}

/**
 * Fractale symétrique à N bougies : un swing high à l'index i n'est marqué que si high[i] est le
 * maximum strict (première occurrence en cas d'égalité, comme `np.argmax`) de la fenêtre
 * [i-left, i+right]. Un swing n'est donc "connu" qu'une fois les `right` bougies suivantes closes
 * — jamais d'information du futur (pas de repaint).
 */
function detectSwings(bars: CtraderTrendbar[], { left, right }: FractalOptions): SwingSeries {
  const n = bars.length;
  const highs: (number | undefined)[] = new Array(n).fill(undefined);
  const lows: (number | undefined)[] = new Array(n).fill(undefined);

  for (let i = left; i < n - right; i++) {
    let maxVal = -Infinity;
    let maxIdx = -1;
    let minVal = Infinity;
    let minIdx = -1;
    for (let k = i - left; k <= i + right; k++) {
      if (bars[k]!.high > maxVal) {
        maxVal = bars[k]!.high;
        maxIdx = k;
      }
      if (bars[k]!.low < minVal) {
        minVal = bars[k]!.low;
        minIdx = k;
      }
    }
    if (maxIdx === i) highs[i] = bars[i]!.high;
    if (minIdx === i) lows[i] = bars[i]!.low;
  }

  return { highs, lows };
}

function trueRangeSeries(bars: CtraderTrendbar[]): number[] {
  const tr: number[] = [0];
  for (let i = 1; i < bars.length; i++) {
    tr.push(
      Math.max(
        bars[i]!.high - bars[i]!.low,
        Math.abs(bars[i]!.high - bars[i - 1]!.close),
        Math.abs(bars[i]!.low - bars[i - 1]!.close),
      ),
    );
  }
  return tr;
}

function rollingMean(values: number[], period: number): (number | undefined)[] {
  const result: (number | undefined)[] = new Array(values.length).fill(undefined);
  let sum = 0;
  for (let i = 0; i < values.length; i++) {
    sum += values[i]!;
    if (i >= period) sum -= values[i - period]!;
    if (i >= period - 1) result[i] = sum / period;
  }
  return result;
}

/** ATR(period) le plus récent, échelle brute x10^5 comme le reste de ce fichier (cf. commentaire de
 * tête de trading.ts pour la conversion vers un prix affiché). `undefined` tant que l'historique est
 * plus court que `period` — même convention que `rollingMean`/`computeAdx`. */
export function computeAtr(bars: CtraderTrendbar[], period = 14): number | undefined {
  return rollingMean(trueRangeSeries(bars), period).at(-1);
}

/** Bougie "molle" : range < displacementMult × ATR courant — pas assez d'order flow réel derrière
 * la cassure. Permissif (true) tant que l'ATR n'a pas assez de bougies pour être calculé, comme la
 * référence. */
function hasDisplacement(
  bars: CtraderTrendbar[],
  i: number,
  atr: (number | undefined)[],
  displacementMult: number,
): boolean {
  const a = atr[i];
  if (a === undefined) return true;
  const candleRange = bars[i]!.high - bars[i]!.low;
  return candleRange >= displacementMult * a;
}

// ─── Méthode 1 — structurelle ───────────────────────────────────────────────

export interface StructuralOptions extends FractalOptions {
  /** Filtre les swings trop petits (bruit) en % du prix — évite qu'un micro-pivot fausse la
   * lecture HH/HL sur un TF bas (M5). 0 = désactivé. */
  minSwingPct?: number;
}

export function classifyStructuralTrend(
  bars: CtraderTrendbar[],
  { left, right, minSwingPct = 0 }: StructuralOptions,
): Trend {
  const { highs, lows } = detectSwings(bars, { left, right });
  const highSeq: number[] = [];
  const lowSeq: number[] = [];
  let current: Trend = 0;

  for (let i = 0; i < bars.length; i++) {
    const h = highs[i];
    if (h !== undefined) {
      const prev = highSeq.at(-1);
      if (prev === undefined || (Math.abs(h - prev) / prev) * 100 >= minSwingPct) highSeq.push(h);
    }
    const l = lows[i];
    if (l !== undefined) {
      const prev = lowSeq.at(-1);
      if (prev === undefined || (Math.abs(l - prev) / prev) * 100 >= minSwingPct) lowSeq.push(l);
    }

    if (highSeq.length >= 2 && lowSeq.length >= 2) {
      const higherHigh = highSeq.at(-1)! > highSeq.at(-2)!;
      const higherLow = lowSeq.at(-1)! > lowSeq.at(-2)!;
      const lowerHigh = highSeq.at(-1)! < highSeq.at(-2)!;
      const lowerLow = lowSeq.at(-1)! < lowSeq.at(-2)!;
      current = higherHigh && higherLow ? 1 : lowerHigh && lowerLow ? -1 : 0;
    }
  }

  return current;
}

// ─── Méthode 2 — événementielle ─────────────────────────────────────────────

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
  const atr = rollingMean(trueRangeSeries(bars), atrPeriod);

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

// ─── Liquidity sweep ─────────────────────────────────────────────────────

export interface SweepState {
  /** Mèche au-delà du dernier swing high puis clôture repassée en dessous — piège vendeur. */
  sweepHigh: boolean;
  /** Symétrique, piège acheteur. */
  sweepLow: boolean;
}

/** État du sweep sur la dernière bougie seulement (le panneau affiche "maintenant", pas un
 * historique) — équivalent à lire `.iloc[-1]` des deux séries pandas de la référence. */
export function detectLatestSweep(
  bars: CtraderTrendbar[],
  { left, right }: FractalOptions,
): SweepState {
  const { highs, lows } = detectSwings(bars, { left, right });
  let lastHigh: number | undefined;
  let lastLow: number | undefined;
  let sweepHigh = false;
  let sweepLow = false;

  for (let i = 0; i < bars.length; i++) {
    if (highs[i] !== undefined) lastHigh = highs[i];
    if (lows[i] !== undefined) lastLow = lows[i];

    sweepHigh = lastHigh !== undefined && bars[i]!.high > lastHigh && bars[i]!.close < lastHigh;
    sweepLow = lastLow !== undefined && bars[i]!.low < lastLow && bars[i]!.close > lastLow;
  }

  return { sweepHigh, sweepLow };
}

// ─── Filtres classiques (pas du SMC) — EMA stack + ADX ──────────────────────

function computeEma(values: number[], span: number): number {
  const alpha = 2 / (span + 1);
  let ema = values[0]!;
  for (let i = 1; i < values.length; i++) ema = values[i]! * alpha + ema * (1 - alpha);
  return ema;
}

/** EMA rapide > EMA lente sur la clôture — direction "classique", pas du SMC. `undefined` si pas
 * assez de bougies pour la plus lente des deux. */
export function computeEmaStack(
  bars: CtraderTrendbar[],
  { fast = 20, slow = 50 }: { fast?: number; slow?: number } = {},
): boolean | undefined {
  if (bars.length < slow) return undefined;
  const closes = bars.map((b) => b.close);
  return computeEma(closes, fast) > computeEma(closes, slow);
}

/**
 * ADX(period) façon Wilder simplifié — moyenne mobile simple plutôt que le lissage RMA classique,
 * comme la référence. Sert de garde-fou pour éviter de lire un signal structurel valide dans un
 * marché qui, statistiquement, ne trend pas (cf. ADX_TRENDING_THRESHOLD). `undefined` si pas assez
 * de bougies.
 */
export function computeAdx(bars: CtraderTrendbar[], period = 14): number | undefined {
  if (bars.length < period * 2 + 1) return undefined;

  const plusDm: number[] = [0];
  const minusDm: number[] = [0];
  for (let i = 1; i < bars.length; i++) {
    const upMove = bars[i]!.high - bars[i - 1]!.high;
    const downMove = bars[i - 1]!.low - bars[i]!.low;
    let plus = Math.max(upMove, 0);
    let minus = Math.max(downMove, 0);
    if (plus - minus <= 0) plus = 0;
    if (minus - plus <= 0) minus = 0;
    plusDm.push(plus);
    minusDm.push(minus);
  }

  const atr = rollingMean(trueRangeSeries(bars), period);
  const plusDmAvg = rollingMean(plusDm, period);
  const minusDmAvg = rollingMean(minusDm, period);

  const dx = bars.map((_, i) => {
    const a = atr[i];
    const pd = plusDmAvg[i];
    const md = minusDmAvg[i];
    if (a === undefined || pd === undefined || md === undefined || a === 0) return 0;
    const plusDi = 100 * (pd / a);
    const minusDi = 100 * (md / a);
    const sum = plusDi + minusDi;
    return sum === 0 ? 0 : (100 * Math.abs(plusDi - minusDi)) / sum;
  });

  return rollingMean(dx, period).at(-1);
}

// ─── Combiné ────────────────────────────────────────────────────────────

export interface TrendComputeOptions extends StructureDetectionOptions {
  minSwingPct?: number;
  emaFast?: number;
  emaSlow?: number;
  adxPeriod?: number;
}

export interface TrendState {
  structural: Trend;
  rawEvent: Trend;
  confirmedEvent: Trend;
  lastEventDisplacementOk: boolean | undefined;
  sweepHigh: boolean;
  sweepLow: boolean;
  emaStackBullish: boolean | undefined;
  adx: number | undefined;
  /** Prochains niveaux de structure (résistance/support) encore surveillés, avec ce que leur
   * cassure produirait — BOS ou CHoCH selon la tendance actuelle. */
  pending: PendingLevels;
}

const EMPTY_TREND_STATE: TrendState = {
  structural: 0,
  rawEvent: 0,
  confirmedEvent: 0,
  lastEventDisplacementOk: undefined,
  sweepHigh: false,
  sweepLow: false,
  emaStackBullish: undefined,
  adx: undefined,
  pending: { resistance: undefined, support: undefined },
};

export function computeTrendState(
  bars: CtraderTrendbar[],
  options: TrendComputeOptions,
): TrendState {
  const {
    left,
    right,
    minSwingPct = 0,
    atrPeriod = 14,
    displacementMult = 1.2,
    emaFast = 20,
    emaSlow = 50,
    adxPeriod = 14,
  } = options;

  if (bars.length < left + right + 1) return EMPTY_TREND_STATE;

  const structural = classifyStructuralTrend(bars, { left, right, minSwingPct });
  const { events, pending } = detectStructureEvents(bars, {
    left,
    right,
    atrPeriod,
    displacementMult,
  });
  const { raw, confirmed, lastDisplacementOk } = classifyEventTrend(events);
  const { sweepHigh, sweepLow } = detectLatestSweep(bars, { left, right });

  return {
    structural,
    rawEvent: raw,
    confirmedEvent: confirmed,
    lastEventDisplacementOk: lastDisplacementOk,
    sweepHigh,
    sweepLow,
    emaStackBullish: computeEmaStack(bars, { fast: emaFast, slow: emaSlow }),
    adx: computeAdx(bars, adxPeriod),
    pending,
  };
}
