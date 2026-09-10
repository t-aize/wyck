/**
 * Référentiel de marché : ce qui se trade (symboles, assets) et comment ça cote
 * (spots, bougies). Aucune idée d'un book ouvert ni d'un ordre.
 *
 * Les prix spots / OHLC arrivent en entier × 10⁵ (ex. `410177000` → `4101.77`).
 * La conversion d'affichage vit côté app (`PRICE_SCALE`).
 */

import { z } from "zod";
import { TRENDBAR_PERIODS, type TrendbarPeriod } from "../protocol/period.ts";

/** Un symbole cTrader tel que `get_symbols` le renvoie. */
export const CtraderSymbolSchema = z.object({
  symbolId: z.number(),
  /** Ticker broker (`XAUUSD`, `US100.r`…) — suffixe conservé. */
  symbolName: z.string(),
  enabled: z.boolean(),
  /** Asset de base (cf. {@link CtraderAsset.assetId}). */
  baseAssetId: z.number(),
  /** Asset de cotation. */
  quoteAssetId: z.number(),
  symbolCategoryId: z.number(),
  description: z.string(),
});
/** @see CtraderSymbolSchema */
export type CtraderSymbol = z.infer<typeof CtraderSymbolSchema>;

/** Enveloppe `get_symbols`. */
export const GetSymbolsResultSchema = z.object({ symbols: z.array(CtraderSymbolSchema) });
/** @see GetSymbolsResultSchema */
export type GetSymbolsResult = z.infer<typeof GetSymbolsResultSchema>;

/** Un asset (devise, métal, indice…) référencé par `baseAssetId` / `quoteAssetId`. */
export const CtraderAssetSchema = z.object({
  assetId: z.number(),
  /** Code court (`XAU`, `USD`, `EUR`). */
  name: z.string(),
  displayName: z.string(),
});
/** @see CtraderAssetSchema */
export type CtraderAsset = z.infer<typeof CtraderAssetSchema>;

/** Enveloppe `get_assets`. */
export const GetAssetsResultSchema = z.object({ assets: z.array(CtraderAssetSchema) });
/** @see GetAssetsResultSchema */
export type GetAssetsResult = z.infer<typeof GetAssetsResultSchema>;

/** Params sortants de `get_spot_prices`. */
export interface GetSpotPricesParams {
  /** IDs des symboles dont on veut bid/ask. */
  symbolId: number[];
}

/** Un tick spot. Prix en entier × 10⁵. */
export const CtraderSpotPriceSchema = z.object({
  symbolId: z.number(),
  /** Prix à l'échelle × 10⁵ (ex. 410177000 → 4101.77). */
  bid: z.number(),
  ask: z.number(),
  high: z.number(),
  low: z.number(),
  sessionClose: z.number(),
  /** Epoch ms. */
  timestamp: z.number(),
});
/** @see CtraderSpotPriceSchema */
export type CtraderSpotPrice = z.infer<typeof CtraderSpotPriceSchema>;

/** Enveloppe `get_spot_prices`. */
export const GetSpotPricesResultSchema = z.object({ prices: z.array(CtraderSpotPriceSchema) });
/** @see GetSpotPricesResultSchema */
export type GetSpotPricesResult = z.infer<typeof GetSpotPricesResultSchema>;

/**
 * Params sortants de `get_trendbars`.
 *
 * Combinaisons valides côté serveur :
 *
 * - `(count)` → N dernières bougies ;
 * - `(toTimestamp, count)` → N bougies se terminant à `toTimestamp` ;
 * - `(fromTimestamp, toTimestamp)` → toutes les bougies sur la plage (≤ 720 h).
 *
 * En pratique **seule la 3e** s'est montrée fiable : les deux autres ont
 * renvoyé une 400 malgré une requête conforme au schéma annoncé.
 */
export interface GetTrendbarsParams {
  symbolId: number;
  period: TrendbarPeriod;
  /** ISO-8601, bornes de la plage (avec `toTimestamp`). */
  fromTimestamp?: string;
  toTimestamp?: string;
  /** Nombre de bougies — peu fiable seul, cf. commentaire d'interface. */
  count?: number;
}

/** Une bougie OHLC. Prix en entier × 10⁵. */
export const CtraderTrendbarSchema = z.object({
  /** Epoch ms de l'ouverture. */
  timestamp: z.number(),
  open: z.number(),
  high: z.number(),
  low: z.number(),
  close: z.number(),
  volume: z.number(),
});
/** @see CtraderTrendbarSchema */
export type CtraderTrendbar = z.infer<typeof CtraderTrendbarSchema>;

/** Enveloppe `get_trendbars`. */
export const GetTrendbarsResultSchema = z.object({
  trendbars: z.array(CtraderTrendbarSchema),
  symbolId: z.number(),
  period: z.enum(TRENDBAR_PERIODS),
});
/** @see GetTrendbarsResultSchema */
export type GetTrendbarsResult = z.infer<typeof GetTrendbarsResultSchema>;
