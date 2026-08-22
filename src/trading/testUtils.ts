import { Cause, Effect, Exit } from "effect";
import type { CtraderClient } from "../ctrader/client.ts";

/** Exécute un Effect purement synchrone (aucune I/O) et retourne sa valeur de succès, ou fait
 * échouer le test si l'Effect a en fait échoué — évite de dupliquer ce dépliage `Exit` dans chaque
 * fichier de test qui exerce `trading/`. */
export function runOk<A, E>(effect: Effect.Effect<A, E>): A {
  const exit = Effect.runSyncExit(effect);
  if (Exit.isFailure(exit)) {
    throw new Error(`expected success, got failure: ${Cause.pretty(exit.cause)}`);
  }
  return exit.value;
}

/** Comme `runOk`, mais attend un échec et retourne l'erreur portée. */
export function runFail<A, E>(effect: Effect.Effect<A, E>): E {
  const exit = Effect.runSyncExit(effect);
  if (Exit.isSuccess(exit)) {
    throw new Error(`expected failure, got success: ${JSON.stringify(exit.value)}`);
  }
  return Cause.squash(exit.cause) as E;
}

/** Mock minimal de `CtraderClient` — seules `getSpotPrices`/`getBalance` sont jamais appelées côté
 * `trading/` (cf. context.ts#fetchTradeContext). `CtraderClient` a des champs privés (cf.
 * ctrader/client.ts), donc pas structurellement compatible avec un simple objet littéral : cast
 * assumé, réservé aux tests. */
export function fakeCtraderClient(
  opts: { prices?: { bid: number; ask: number }[]; equity?: number; moneyDigits?: number } = {},
): CtraderClient {
  const {
    prices = [{ bid: 200_000_000, ask: 200_100_000 }],
    equity = 1_000_000,
    moneyDigits = 2,
  } = opts;
  return {
    getSpotPrices: () =>
      Effect.succeed({
        prices: prices.map((p, i) => ({
          symbolId: i,
          bid: p.bid,
          ask: p.ask,
          high: 0,
          low: 0,
          sessionClose: 0,
          timestamp: 0,
        })),
      }),
    getBalance: () =>
      Effect.succeed({
        balance: equity,
        equity,
        freeMargin: equity,
        balanceVersion: 1,
        moneyDigits,
        depositAssetId: 1,
      }),
  } as unknown as CtraderClient;
}
