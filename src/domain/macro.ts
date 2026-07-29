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

// ─── FRED (dollar large, taux réel 10 ans) ─────────────────────────────────

/**
 * DTWEXBGS = "Nominal Broad U.S. Dollar Index" de la Fed — PAS le DXY (ICE), qui est un indice
 * propriétaire à accès payant, non disponible via une API publique gratuite. Base de calcul et
 * panier de devises différents du DXY (2006=100, panier plus large) ⇒ échelle différente (~120
 * ici contre ~95-105 pour le DXY) : ne pas confondre les deux valeurs. C'est la meilleure
 * approximation gratuite d'un indicateur de force du dollar, pas un substitut exact au DXY.
 */
export const USD_BROAD_SERIES_ID = "DTWEXBGS";
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

/** Vérifie qu'une clé FRED fonctionne réellement (rejette en cas de 400) — utilisé par SetupScreen, même logique que la vérification de connexion cTrader (client.getBalance()). */
export async function validateFredApiKey(apiKey: string): Promise<void> {
  await fetchFredSeries(USD_BROAD_SERIES_ID, apiKey);
}

// ─── Combiné, avec cache disque ─────────────────────────────────────────────

export interface MacroSnapshot {
  cot?: CotSnapshot;
  /** Nominal Broad U.S. Dollar Index (Fed) — pas le DXY (ICE), cf. commentaire sur USD_BROAD_SERIES_ID. */
  usdBroad?: FredSnapshot;
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
  usdBroad: FredSnapshotSchema.optional(),
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
 * semaine (vendredi), dollar large/real yield une fois par jour ouvré — même choix que
 * fetchCalendar dans news.ts. Chaque source échoue indépendamment et retombe sur la dernière
 * valeur en cache plutôt que de faire échouer les deux autres.
 */
export async function fetchMacro(
  fredApiKey: string,
  options: { force?: boolean } = {},
): Promise<MacroSnapshot> {
  const cached = await readCache();
  if (
    !options.force &&
    cached &&
    parisDayKey(new Date(cached.fetchedAt)) === parisDayKey(new Date())
  ) {
    return { cot: cached.cot, usdBroad: cached.usdBroad, realYield: cached.realYield };
  }

  const [cot, usdBroad, realYield] = await Promise.all([
    fetchCot().catch(() => cached?.cot),
    fetchFredSeries(USD_BROAD_SERIES_ID, fredApiKey).catch(() => cached?.usdBroad),
    fetchFredSeries(REAL_YIELD_SERIES_ID, fredApiKey).catch(() => cached?.realYield),
  ]);

  const snapshot: MacroSnapshot = { cot, usdBroad, realYield };
  await writeCache(snapshot).catch(() => {});
  return snapshot;
}
