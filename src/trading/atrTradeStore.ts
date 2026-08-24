/** Ordres ATR en attente encore à suivre, avec cache/persistance disque — même convention que
 * `news/calendar.ts` (Effect + `FileSystem`, zod, best-effort). Alimenté par `useTradeConfirm.ts` à
 * la confirmation d'un trade en mode ATR, consommé par `useAtrAutoRefresh.ts` toutes les 60s. */

import { join } from "node:path";
import { FileSystem } from "@effect/platform";
import { Effect } from "effect";
import { z } from "zod";
import { APP_DATA_DIR } from "../constants.ts";
import { TradeSideSchema } from "../ctrader/schemas.ts";

const STORE_PATH = join(APP_DATA_DIR, "atr-trades.json");

const AtrTradeRecordSchema = z.object({
  orderId: z.number(),
  tradeSide: TradeSideSchema,
  rewardRiskRatio: z.number(),
});
export type AtrTradeRecord = z.infer<typeof AtrTradeRecordSchema>;

const StoreFileSchema = z.object({ trades: z.array(AtrTradeRecordSchema) });

/** `[]` si absent, corrompu, ou d'un format antérieur — jamais en échec, même logique que
 * `calendar.ts#readCache`/`config.ts#readConfig` : un fichier illisible n'est pas plus fatal
 * qu'un fichier absent. */
function readAll(): Effect.Effect<AtrTradeRecord[], never, FileSystem.FileSystem> {
  return Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const raw = yield* fs.readFileString(STORE_PATH).pipe(Effect.orElseSucceed(() => undefined));
    if (raw === undefined) return [];

    return yield* Effect.try(() => StoreFileSchema.parse(JSON.parse(raw)).trades).pipe(
      Effect.orElseSucceed(() => [] as AtrTradeRecord[]),
    );
  });
}

/** Écriture best-effort (avalée ici, pas par l'appelant) : un échec ne doit jamais faire échouer
 * un envoi d'ordre par ailleurs réussi (cf. useTradeConfirm.ts) ni bloquer une passe de refresh. */
function writeAll(trades: AtrTradeRecord[]): Effect.Effect<void, never, FileSystem.FileSystem> {
  return Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    if (!(yield* fs.exists(APP_DATA_DIR))) {
      yield* fs.makeDirectory(APP_DATA_DIR, { recursive: true });
    }
    yield* fs.writeFileString(STORE_PATH, JSON.stringify({ trades }, null, 2));
  }).pipe(Effect.ignore);
}

export function readAtrTrades(): Effect.Effect<AtrTradeRecord[], never, FileSystem.FileSystem> {
  return readAll();
}

/** Lecture-modification-écriture (jamais un écrasement aveugle) : remplace un enregistrement
 * existant pour le même `orderId` s'il y en a un, l'ajoute sinon. */
export function recordAtrTrade(
  record: AtrTradeRecord,
): Effect.Effect<void, never, FileSystem.FileSystem> {
  return Effect.gen(function* () {
    const trades = yield* readAll();
    yield* writeAll([...trades.filter((t) => t.orderId !== record.orderId), record]);
  });
}

/** No-op silencieux si aucun des `orderIds` n'était suivi. */
export function removeAtrTrades(
  orderIds: number[],
): Effect.Effect<void, never, FileSystem.FileSystem> {
  if (orderIds.length === 0) return Effect.void;
  return Effect.gen(function* () {
    const trades = yield* readAll();
    const remaining = trades.filter((t) => !orderIds.includes(t.orderId));
    if (remaining.length !== trades.length) yield* writeAll(remaining);
  });
}
