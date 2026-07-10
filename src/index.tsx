import { createCliRenderer } from "@opentui/core";
import { createRoot } from "@opentui/react";
import { useEffect, useMemo, useState } from "react";
import { CtraderClient, type GetPositionsResult } from "./ctrader-client.ts";
import { env } from "./env.ts";
import { type CalendarEvent, fetchCalendar } from "./news.ts";
import {
  formatTradeSummary,
  type PreparedTrade,
  parseTradeCommand,
  prepareTrade,
  toCreateOrderParams,
} from "./trading.ts";
import { CommandBar, type Feedback } from "./ui/CommandBar.tsx";
import { useClock, useInterval, useTerminalShortcuts } from "./ui/hooks.ts";
import { NewsPanel } from "./ui/NewsPanel.tsx";
import { PositionsPanel } from "./ui/PositionsPanel.tsx";
import { PriceHeader } from "./ui/PriceHeader.tsx";
import { TradeConfirmModal } from "./ui/TradeConfirmModal.tsx";
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
  const [feedback, setFeedback] = useState<Feedback>({
    kind: "info",
    message: "tapez help pour la liste des commandes",
  });
  const [pendingTrade, setPendingTrade] = useState<PreparedTrade>();

  useTerminalShortcuts(setFeedback);

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
    const trimmed = raw.trim();
    if (!trimmed) return;
    const [commandRaw, ...args] = trimmed.split(/\s+/);
    const command = commandRaw?.toLowerCase() ?? "";

    switch (command) {
      case "help":
        setFeedback({ kind: "info", message: "commandes : trade  refresh  clear  help" });
        return;
      case "refresh":
        setFeedback({ kind: "info", message: "actualisation…" });
        void Promise.all([refreshMarket(), refreshNews({ force: true })]).then(() => {
          setFeedback({ kind: "success", message: "actualisé" });
        });
        return;
      case "clear":
        setFeedback({ kind: "info", message: "" });
        return;
      case "trade": {
        if (!symbolId) {
          setFeedback({ kind: "error", message: "pas encore connecté au serveur" });
          return;
        }
        const parsed = parseTradeCommand(args);
        if (typeof parsed === "string") {
          setFeedback({ kind: "error", message: parsed });
          return;
        }
        setFeedback({ kind: "info", message: "calcul en cours…" });
        void prepareTrade(client, symbolId, parsed).then(
          (trade) => {
            setPendingTrade(trade);
            setFeedback({ kind: "info", message: "trade calculé — confirme dans la popup" });
          },
          (error) =>
            setFeedback({
              kind: "error",
              message: error instanceof Error ? error.message : String(error),
            }),
        );
        return;
      }
      default:
        setFeedback({
          kind: "error",
          message: `commande inconnue : ${command} — help pour la liste`,
        });
    }
  }

  function confirmPendingTrade() {
    if (!pendingTrade || !symbolId) return;
    const summary = formatTradeSummary(pendingTrade);
    setPendingTrade(undefined);
    setFeedback({ kind: "info", message: "envoi de l'ordre…" });
    void client.createOrder(toCreateOrderParams(symbolId, pendingTrade)).then(
      () => setFeedback({ kind: "success", message: `ordre envoyé : ${summary}` }),
      (error) =>
        setFeedback({
          kind: "error",
          message: `échec envoi : ${error instanceof Error ? error.message : String(error)}`,
        }),
    );
  }

  function cancelPendingTrade() {
    setPendingTrade(undefined);
    setFeedback({ kind: "info", message: "trade annulé" });
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
      <NewsPanel events={calendar} errorMessage={newsError} now={now} />
      <CommandBar feedback={feedback} onSubmit={runCommand} focused={!pendingTrade} />
      {pendingTrade && (
        <TradeConfirmModal
          trade={pendingTrade}
          onConfirm={confirmPendingTrade}
          onCancel={cancelPendingTrade}
        />
      )}
    </box>
  );
}

if (import.meta.main) {
  const renderer = await createCliRenderer({
    backgroundColor: theme.bg,
    // Ctrl+C est géré nous-mêmes (double appui, cf. useTerminalShortcuts). `exitOnCtrlC: false`
    // ne suffit pas seul : SIGINT (déclenché par Ctrl+C selon le terminal) a son propre chemin
    // de sortie via `exitSignals`, indépendant — vérifié en pratique, sans ce retrait le premier
    // Ctrl+C ferme quand même l'appli en coupant le parsing clavier avant le second appui.
    exitOnCtrlC: false,
    exitSignals: ["SIGTERM", "SIGQUIT", "SIGABRT", "SIGHUP", "SIGBREAK", "SIGPIPE", "SIGBUS"],
  });
  createRoot(renderer).render(<App />);
}
