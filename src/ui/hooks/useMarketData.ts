import { Effect } from "effect";
import { useEffect, useMemo, useState } from "react";
import type { CtraderClientLive, GetPositionsResult } from "../../ctrader/client.ts";
import { toMessage } from "../../errors.ts";
import { useInterval } from "./useInterval.ts";

const PRICE_POLL_MS = 3_000;
/** ~1min de tendance à 3s/tick (cf. PRICE_POLL_MS) — assez pour un sparkline lisible sans manger
 * toute la largeur du header. */
const PRICE_HISTORY_LENGTH = 20;

export interface MarketData {
  bid: number | undefined;
  ask: number | undefined;
  /** Prix moyen (bid+ask)/2, un point par poll, plafonné à PRICE_HISTORY_LENGTH — pour le
   * sparkline de tendance dans PriceHeader. */
  priceHistory: number[];
  positions: GetPositionsResult | undefined;
  /** Capital du compte, entier à l'échelle `moneyDigits` (cf. formatMoney) */
  balance: number | undefined;
  moneyDigits: number | undefined;
  refreshMarket: () => Promise<void>;
}

/** `onError` partage l'état `connectionError` de useCtraderConnection (même affichage). */
export function useMarketData(
  client: CtraderClientLive,
  symbolId: number | undefined,
  onError: (message: string | undefined) => void,
): MarketData {
  const [bid, setBid] = useState<number>();
  const [ask, setAsk] = useState<number>();
  const [priceHistory, setPriceHistory] = useState<number[]>([]);
  const [positions, setPositions] = useState<GetPositionsResult>();
  const [balance, setBalance] = useState<number>();
  const [moneyDigits, setMoneyDigits] = useState<number>();

  const refreshMarket = useMemo(
    () => async () => {
      if (!symbolId) return;
      try {
        const [spot, pos, bal] = await Promise.all([
          Effect.runPromise(client.getSpotPrices({ symbolId: [symbolId] })),
          Effect.runPromise(client.getPositions()),
          Effect.runPromise(client.getBalance()),
        ]);
        const price = spot.prices[0];
        if (price) {
          setBid(price.bid);
          setAsk(price.ask);
          const mid = (price.bid + price.ask) / 2;
          setPriceHistory((history) => [...history, mid].slice(-PRICE_HISTORY_LENGTH));
        }
        setPositions(pos);
        setBalance(bal.balance);
        setMoneyDigits(bal.moneyDigits);
        onError(undefined);
      } catch (error) {
        onError(toMessage(error));
      }
    },
    [client, symbolId, onError],
  );

  useInterval(refreshMarket, PRICE_POLL_MS);

  // `useInterval` déclenche son premier appel immédiat au montage, donc avant que `symbolId`
  // soit connu (connect() est async) — ce premier appel n'a aucun effet. Sans ce second effet,
  // positions/ordres n'apparaissent qu'au prochain tick (jusqu'à PRICE_POLL_MS de retard) après
  // la connexion, au lieu d'être visibles immédiatement.
  useEffect(() => {
    if (symbolId) void refreshMarket();
  }, [symbolId, refreshMarket]);

  return { bid, ask, priceHistory, positions, balance, moneyDigits, refreshMarket };
}
