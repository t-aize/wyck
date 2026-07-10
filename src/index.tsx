import { createCliRenderer } from "@opentui/core";
import { createRoot } from "@opentui/react";
import { useEffect, useMemo, useState } from "react";
import { CtraderClient, type GetPositionsResult } from "./ctrader-client.ts";
import { env } from "./env.ts";
import { type CalendarEvent, fetchCalendar } from "./news.ts";
import { CommandBar } from "./ui/CommandBar.tsx";
import { useClock, useInterval } from "./ui/hooks.ts";
import { NewsPanel } from "./ui/NewsPanel.tsx";
import { PositionsPanel } from "./ui/PositionsPanel.tsx";
import { PriceHeader } from "./ui/PriceHeader.tsx";
import { theme } from "./ui/theme.ts";

const PRICE_POLL_MS = 3_000;
const NEWS_POLL_MS = 5 * 60_000;

export function App() {
  const now = useClock();
  const [client] = useState(() => new CtraderClient());
  const [connected, setConnected] = useState(false);
  const [connectionError, setConnectionError] = useState<string>();
  const [symbolId, setSymbolId] = useState<number>();
  const [bid, setBid] = useState<number>();
  const [ask, setAsk] = useState<number>();
  const [positions, setPositions] = useState<GetPositionsResult>();
  const [calendar, setCalendar] = useState<CalendarEvent[]>([]);
  const [newsError, setNewsError] = useState<string>();
  const [goldOnly, setGoldOnly] = useState(false);
  const [feedback, setFeedback] = useState("tapez /help pour la liste des commandes");

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      try {
        await client.connect();
        const { symbols } = await client.getSymbols();
        const symbol = symbols.find((s) => s.symbolName === env.SYMBOL);
        if (!symbol) throw new Error(`Symbole ${env.SYMBOL} introuvable côté serveur`);
        if (cancelled) return;
        setSymbolId(symbol.symbolId);
        setConnected(true);
      } catch (error) {
        if (!cancelled) setConnectionError(error instanceof Error ? error.message : String(error));
      }
    })();

    return () => {
      cancelled = true;
      void client.close();
    };
  }, [client]);

  const refreshMarket = useMemo(
    () => async () => {
      if (!symbolId) return;
      try {
        const [spot, pos] = await Promise.all([
          client.getSpotPrices({ symbolId: [symbolId] }),
          client.getPositions(),
        ]);
        const price = spot.prices[0];
        if (price) {
          setBid(price.bid);
          setAsk(price.ask);
        }
        setPositions(pos);
        setConnectionError(undefined);
      } catch (error) {
        setConnectionError(error instanceof Error ? error.message : String(error));
      }
    },
    [client, symbolId],
  );

  useInterval(refreshMarket, PRICE_POLL_MS);

  const refreshNews = useMemo(
    () =>
      async (options: { force?: boolean } = {}) => {
        try {
          setCalendar(await fetchCalendar(options));
          setNewsError(undefined);
        } catch (error) {
          setNewsError(error instanceof Error ? error.message : String(error));
        }
      },
    [],
  );

  useInterval(() => void refreshNews(), NEWS_POLL_MS);

  function runCommand(raw: string) {
    const command = raw.trim();
    if (!command) return;

    switch (command) {
      case "/help":
        setFeedback("commandes : /refresh  /gold  /clear  /help");
        return;
      case "/refresh":
        setFeedback("actualisation…");
        void refreshMarket();
        void refreshNews({ force: true });
        return;
      case "/gold": {
        const next = !goldOnly;
        setGoldOnly(next);
        setFeedback(`filtre or : ${next ? "activé" : "désactivé"}`);
        return;
      }
      case "/clear":
        setFeedback("");
        return;
      default:
        setFeedback(`commande inconnue : ${command} — /help pour la liste`);
    }
  }

  return (
    <box
      style={{ flexDirection: "column", width: "100%", height: "100%", backgroundColor: theme.bg }}
    >
      <PriceHeader
        symbol={env.SYMBOL}
        bid={bid}
        ask={ask}
        connected={connected}
        now={now}
        errorMessage={connectionError}
      />
      <PositionsPanel positions={positions} now={now} />
      <NewsPanel events={calendar} errorMessage={newsError} goldOnly={goldOnly} now={now} />
      <CommandBar feedback={feedback} onSubmit={runCommand} />
    </box>
  );
}

if (import.meta.main) {
  const renderer = await createCliRenderer({ backgroundColor: theme.bg });
  createRoot(renderer).render(<App />);
}
