import { Effect } from "effect";
import { PRICE_SCALE } from "../constants.ts";
import type { CtraderClient, CtraderMcpError } from "../ctrader/client.ts";
import { computeStructure, type StructureReading } from "./bias.ts";
import { detectSwings } from "./swings.ts";
import type { StructureBar } from "./types.ts";

export const STRUCTURE_PERIODS = ["M_1", "M_5", "M_15", "H_1"] as const;
export type StructurePeriod = (typeof STRUCTURE_PERIODS)[number];

const PERIOD_MINUTES: Record<StructurePeriod, number> = { M_1: 1, M_5: 5, M_15: 15, H_1: 60 };

/** ~200 bougies clôturées par timeframe — assez pour plusieurs paires de swings confirmées ; en
 * dessous, le biais retombe naturellement sur "neutral" faute de swings à casser (cf. bias.ts),
 * pas besoin d'un garde-fou explicite. */
const LOOKBACK_BARS = 200;

/** Marge sur la fenêtre demandée au-delà du strict nécessaire (bougies manquantes un weekend/jour
 * férié, écarts de trading) — cf. la même prudence dans trading/atr.ts#ATR_WINDOW_MS. */
const WINDOW_SAFETY_FACTOR = 1.5;

function fetchTimeframeStructure(
  client: CtraderClient,
  symbolId: number,
  period: StructurePeriod,
): Effect.Effect<StructureReading, CtraderMcpError> {
  return Effect.gen(function* () {
    const now = Date.now();
    const windowMs = PERIOD_MINUTES[period] * 60_000 * LOOKBACK_BARS * WINDOW_SAFETY_FACTOR;
    // (fromTimestamp, toTimestamp) est la seule combinaison de get_trendbars fiable en pratique
    // (cf. ctrader/schemas.ts#GetTrendbarsParams, déjà exploité par trading/atr.ts#fetchAtr) —
    // `count` seul renvoie une erreur 400 côté serveur malgré un schéma de requête valide.
    const { trendbars } = yield* client.getTrendbars({
      symbolId,
      period,
      fromTimestamp: String(now - windowMs),
      toTimestamp: String(now),
    });

    const bars: StructureBar[] = trendbars.map((bar) => ({
      timestamp: bar.timestamp,
      high: bar.high / PRICE_SCALE,
      low: bar.low / PRICE_SCALE,
      close: bar.close / PRICE_SCALE,
    }));

    return computeStructure(bars, detectSwings(bars));
  });
}

/** Structure (biais + résistance/support) sur M1/M5/M15/H1, calculés indépendamment (aucune
 * corrélation entre timeframes) — purement informatif, aucune commande n'en dépend. */
export function fetchStructure(
  client: CtraderClient,
  symbolId: number,
): Effect.Effect<Record<StructurePeriod, StructureReading>, CtraderMcpError> {
  return Effect.gen(function* () {
    const [m1, m5, m15, h1] = yield* Effect.all(
      STRUCTURE_PERIODS.map((period) => fetchTimeframeStructure(client, symbolId, period)),
      { concurrency: "unbounded" },
    );
    return { M_1: m1!, M_5: m5!, M_15: m15!, H_1: h1! };
  });
}
