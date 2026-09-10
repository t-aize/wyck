import type { CtraderOrder, GetPositionsResult } from "@aurum/ctrader";
import { Effect } from "effect";
import { useEffect, useMemo, useState } from "react";
import { PRICE_SCALE } from "../../constants.ts";
import { toMessage } from "../../utils/errors.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { useInterval } from "./useInterval.ts";

const PRICE_POLL_MS = 3_000;
const PRICE_HISTORY_LENGTH = 20;

export interface SpotQuote {
  bid: number;
  ask: number;
  bidPrice: number;
  askPrice: number;
}

interface MarketData {
  bid: number | undefined;
  ask: number | undefined;
  bidPrice: number | undefined;
  askPrice: number | undefined;
  priceHistory: number[];
  spreadHistory: number[];
  quotesBySymbolId: ReadonlyMap<number, SpotQuote>;
  positions: GetPositionsResult | undefined;
  balance: number | undefined;
  moneyDigits: number | undefined;
  refreshMarket: () => Promise<void>;
}

function mergeOrders(fromPositions: CtraderOrder[], fromPending: CtraderOrder[]): CtraderOrder[] {
  const byId = new Map<number, CtraderOrder>();
  for (const order of fromPositions) byId.set(order.orderId, order);
  for (const order of fromPending) byId.set(order.orderId, order);
  return [...byId.values()];
}

export function useMarketData(): MarketData {
  const { client, symbolId, connected, reportConnectionError } = useCtrader();
  const [bid, setBid] = useState<number>();
  const [ask, setAsk] = useState<number>();
  const [priceHistory, setPriceHistory] = useState<number[]>([]);
  const [spreadHistory, setSpreadHistory] = useState<number[]>([]);
  const [quotesBySymbolId, setQuotesBySymbolId] = useState<ReadonlyMap<number, SpotQuote>>(
    new Map(),
  );
  const [positions, setPositions] = useState<GetPositionsResult>();
  const [balance, setBalance] = useState<number>();
  const [moneyDigits, setMoneyDigits] = useState<number>();

  useEffect(() => {
    setBid(undefined);
    setAsk(undefined);
    setPriceHistory([]);
    setSpreadHistory([]);
    if (symbolId === undefined) return;
  }, [symbolId]);

  const refreshMarket = useMemo(
    () => async () => {
      if (!connected) return;
      try {
        const [pos, pendingResult, bal] = await Promise.all([
          Effect.runPromise(client.getPositions()),
          Effect.runPromise(Effect.either(client.getPendingOrders())),
          Effect.runPromise(client.getBalance()),
        ]);
        const pendingOrders = pendingResult._tag === "Right" ? pendingResult.right.orders : [];
        const orders = mergeOrders(pos.orders, pendingOrders);
        const book: GetPositionsResult = { positions: pos.positions, orders };

        const bookSymbolIds = [
          ...book.positions.map((p) => p.symbolId).filter((id): id is number => id !== undefined),
          ...book.orders.map((o) => o.symbolId),
        ];
        const ids = [...new Set([...(symbolId !== undefined ? [symbolId] : []), ...bookSymbolIds])];
        const spot =
          ids.length > 0
            ? await Effect.runPromise(client.getSpotPrices({ symbolId: ids }))
            : { prices: [] };

        const nextQuotes = new Map<number, SpotQuote>();
        for (const price of spot.prices) {
          nextQuotes.set(price.symbolId, {
            bid: price.bid,
            ask: price.ask,
            bidPrice: price.bid / PRICE_SCALE,
            askPrice: price.ask / PRICE_SCALE,
          });
        }
        setQuotesBySymbolId(nextQuotes);

        if (symbolId !== undefined) {
          const selected = nextQuotes.get(symbolId);
          if (selected) {
            setBid(selected.bid);
            setAsk(selected.ask);
            const mid = (selected.bid + selected.ask) / 2;
            setPriceHistory((history) => [...history, mid].slice(-PRICE_HISTORY_LENGTH));
            setSpreadHistory((history) =>
              [...history, selected.ask - selected.bid].slice(-PRICE_HISTORY_LENGTH),
            );
          }
        }
        setPositions(book);
        setBalance(bal.balance);
        setMoneyDigits(bal.moneyDigits);
        reportConnectionError(undefined);
      } catch (error) {
        reportConnectionError(toMessage(error));
      }
    },
    [client, connected, symbolId, reportConnectionError],
  );

  useInterval(refreshMarket, PRICE_POLL_MS);

  useEffect(() => {
    if (connected) void refreshMarket();
  }, [connected, refreshMarket]);

  const bidPrice = useMemo(() => (bid === undefined ? undefined : bid / PRICE_SCALE), [bid]);
  const askPrice = useMemo(() => (ask === undefined ? undefined : ask / PRICE_SCALE), [ask]);

  return {
    bid,
    ask,
    bidPrice,
    askPrice,
    priceHistory,
    spreadHistory,
    quotesBySymbolId,
    positions,
    balance,
    moneyDigits,
    refreshMarket,
  };
}
