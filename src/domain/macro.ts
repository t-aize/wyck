/**
 * Contexte macro pour l'or : positionnement des gros spéculateurs (COT, CFTC — hebdomadaire,
 * sans clé) et dollar/taux réel (FRED — quotidien, clé API gratuite requise). Indicatif, pas un
 * signal de trading — même esprit que goldDirection dans news.ts : on affiche les chiffres et
 * leur tendance, pas une conclusion haussière/baissière (le sens à donner au COT dépend de la
 * lecture de chacun — "smart money" désigne parfois les commerciaux, parfois les spéculatifs
 * selon qui commente).
 */

import { join } from "node:path";
import { z } from "zod";
import { APP_DATA_DIR } from "../constants.ts";
import { parisDayKey } from "./news.ts";

const CACHE_PATH = join(APP_DATA_DIR, "macro-cache.json");

// ─── COT (CFTC, Legacy Futures Only, Gold — COMEX) ─────────────────────────

/** Socrata renvoie les colonnes numériques en chaînes ("224785") — z.coerce absorbe ça. */
const CotRowSchema = z.object({
  report_date_as_yyyy_mm_dd: z.string(),
  noncomm_positions_long_all: z.coerce.number(),
  noncomm_positions_short_all: z.coerce.number(),
  open_interest_all: z.coerce.number(),
});

export interface CotSnapshot {
  reportDate: string;
  /** Position nette des "non-commerciaux" (gros spéculateurs/funds) : long - short. */
  net: number;
  /** Variation vs le rapport hebdomadaire précédent, si disponible. */
  change?: number;
  openInterest: number;
}

/** Rapport le plus récent d'abord (les appelants demandent `$order=...DESC`). */
export function computeCotSnapshot(rows: z.infer<typeof CotRowSchema>[]): CotSnapshot | undefined {
  const latest = rows[0];
  if (!latest) return undefined;
  const net = latest.noncomm_positions_long_all - latest.noncomm_positions_short_all;
  const previous = rows[1];
  const change = previous
    ? net - (previous.noncomm_positions_long_all - previous.noncomm_positions_short_all)
    : undefined;
  return {
    reportDate: latest.report_date_as_yyyy_mm_dd,
    net,
    change,
    openInterest: latest.open_interest_all,
  };
}

function cotUrl(): string {
  const url = new URL("https://publicreporting.cftc.gov/resource/6dca-aqww.json");
  url.searchParams.set("market_and_exchange_names", "GOLD - COMMODITY EXCHANGE INC.");
  url.searchParams.set("$order", "report_date_as_yyyy_mm_dd DESC");
  url.searchParams.set("$limit", "2");
  return url.toString();
}

async function fetchCot(): Promise<CotSnapshot | undefined> {
  const response = await fetch(cotUrl());
  if (!response.ok) throw new Error(`CFTC : HTTP ${response.status}`);
  const rows = z.array(CotRowSchema).parse(await response.json());
  return computeCotSnapshot(rows);
}

// ─── FRED (dollar index, taux réel 10 ans) ─────────────────────────────────

export const DXY_SERIES_ID = "DTWEXBGS";
export const REAL_YIELD_SERIES_ID = "DFII10";

const FredObservationSchema = z.object({ date: z.string(), value: z.string() });
const FredResponseSchema = z.object({ observations: z.array(FredObservationSchema) });

export interface FredSnapshot {
  date: string;
  value: number;
  change?: number;
}

/** FRED marque les jours sans valeur (fériés...) par "." plutôt que d'omettre la ligne. */
export function computeFredSnapshot(
  observations: { date: string; value: string }[],
): FredSnapshot | undefined {
  const valid = observations.filter((o) => o.value !== ".");
  const latest = valid[0];
  if (!latest) return undefined;
  const previous = valid[1];
  return {
    date: latest.date,
    value: Number(latest.value),
    change: previous ? Number(latest.value) - Number(previous.value) : undefined,
  };
}

async function fetchFredSeries(
  seriesId: string,
  apiKey: string,
): Promise<FredSnapshot | undefined> {
  const url = new URL("https://api.stlouisfed.org/fred/series/observations");
  url.searchParams.set("series_id", seriesId);
  url.searchParams.set("api_key", apiKey);
  url.searchParams.set("file_type", "json");
  url.searchParams.set("sort_order", "desc");
  url.searchParams.set("limit", "5");

  const response = await fetch(url);
  if (response.status === 400) throw new Error("FRED : clé API invalide");
  if (!response.ok) throw new Error(`FRED : HTTP ${response.status}`);
  const { observations } = FredResponseSchema.parse(await response.json());
  return computeFredSnapshot(observations);
}

// ─── Combiné, avec cache disque ─────────────────────────────────────────────

export interface MacroSnapshot {
  cot?: CotSnapshot;
  dxy?: FredSnapshot;
  realYield?: FredSnapshot;
}

const CotSnapshotSchema = z.object({
  reportDate: z.string(),
  net: z.number(),
  change: z.number().optional(),
  openInterest: z.number(),
});
const FredSnapshotSchema = z.object({
  date: z.string(),
  value: z.number(),
  change: z.number().optional(),
});
const CacheFileSchema = z.object({
  fetchedAt: z.string(),
  cot: CotSnapshotSchema.optional(),
  dxy: FredSnapshotSchema.optional(),
  realYield: FredSnapshotSchema.optional(),
});

async function readCache(): Promise<(MacroSnapshot & { fetchedAt: string }) | undefined> {
  try {
    const file = Bun.file(CACHE_PATH);
    if (!(await file.exists())) return undefined;
    return CacheFileSchema.parse(await file.json());
  } catch {
    return undefined;
  }
}

async function writeCache(snapshot: MacroSnapshot): Promise<void> {
  const payload = { fetchedAt: new Date().toISOString(), ...snapshot };
  await Bun.write(CACHE_PATH, JSON.stringify(payload, null, 2));
}

/**
 * Un fetch par jour calendaire (Paris) suffit largement : le COT ne change qu'une fois par
 * semaine (vendredi), DXY/real yield une fois par jour ouvré — même choix que fetchCalendar
 * dans news.ts. Chaque source échoue indépendamment et retombe sur la dernière valeur en cache
 * plutôt que de faire échouer les deux autres : `fredApiKey` absent laisse juste dxy/realYield
 * à `undefined` sans empêcher le COT (sans clé, lui) de s'afficher.
 */
export async function fetchMacro(
  fredApiKey: string | undefined,
  options: { force?: boolean } = {},
): Promise<MacroSnapshot> {
  const cached = await readCache();
  if (
    !options.force &&
    cached &&
    parisDayKey(new Date(cached.fetchedAt)) === parisDayKey(new Date())
  ) {
    return { cot: cached.cot, dxy: cached.dxy, realYield: cached.realYield };
  }

  const [cot, dxy, realYield] = await Promise.all([
    fetchCot().catch(() => cached?.cot),
    fredApiKey
      ? fetchFredSeries(DXY_SERIES_ID, fredApiKey).catch(() => cached?.dxy)
      : Promise.resolve(cached?.dxy),
    fredApiKey
      ? fetchFredSeries(REAL_YIELD_SERIES_ID, fredApiKey).catch(() => cached?.realYield)
      : Promise.resolve(cached?.realYield),
  ]);

  const snapshot: MacroSnapshot = { cot, dxy, realYield };
  await writeCache(snapshot).catch(() => {});
  return snapshot;
}
