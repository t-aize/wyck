import { createCliRenderer } from "@opentui/core";
import { createRoot } from "@opentui/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CtraderClient, type CtraderOrder, type GetPositionsResult } from "./ctrader/client.ts";
import { formatTradeSummary, parseModifyCommand, parseTradeCommand } from "./domain/commands.ts";
import { type CalendarEvent, fetchCalendar } from "./domain/news.ts";
import { type PreparedTrade, prepareTrade, toCreateOrderParams } from "./domain/trading.ts";
import { env } from "./env.ts";
import { toMessage } from "./errors.ts";
import { CancelConfirmModal } from "./ui/CancelConfirmModal.tsx";
import { CommandBar, type CommandBarHandle, type Feedback } from "./ui/CommandBar.tsx";
import { useClock, useInterval, useTerminalShortcuts } from "./ui/hooks.ts";
import { ModifyConfirmModal } from "./ui/ModifyConfirmModal.tsx";
import { NewsPanel } from "./ui/NewsPanel.tsx";
import { PositionsPanel } from "./ui/PositionsPanel.tsx";
import { PriceHeader } from "./ui/PriceHeader.tsx";
import { TradeConfirmModal } from "./ui/TradeConfirmModal.tsx";
import { theme } from "./ui/theme.ts";

interface PendingModify {
  order: CtraderOrder;
  stopLoss?: number;
  takeProfit?: number;
}

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
  const [pendingModify, setPendingModify] = useState<PendingModify>();
  const [pendingCancel, setPendingCancel] = useState<CtraderOrder>();
  const commandBarRef = useRef<CommandBarHandle>(null);

  useTerminalShortcuts(
    setFeedback,
    useCallback(() => commandBarRef.current?.clearIfNotEmpty() ?? false, []),
  );

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
        if (!cancelled) setConnectionError(toMessage(error));
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
        setConnectionError(toMessage(error));
      }
    },
    [client, symbolId],
  );

  useInterval(refreshMarket, PRICE_POLL_MS);

  // `useInterval` déclenche son premier appel immédiat au montage, donc avant que `symbolId`
  // soit connu (connect() est async) — ce premier appel n'a aucun effet. Sans ce second effet,
  // positions/ordres n'apparaissent qu'au prochain tick (jusqu'à PRICE_POLL_MS de retard) après
  // la connexion, au lieu d'être visibles immédiatement.
  useEffect(() => {
    if (symbolId) void refreshMarket();
  }, [symbolId, refreshMarket]);

  const refreshNews = useMemo(
    () =>
      async (options: { force?: boolean } = {}) => {
        try {
          setCalendar(await fetchCalendar(options));
          setNewsError(undefined);
        } catch (error) {
          setNewsError(toMessage(error));
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
        setFeedback({
          kind: "info",
          message: "commandes : trade  modify  cancel  refresh  clear  help",
        });
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
          (error) => setFeedback({ kind: "error", message: toMessage(error) }),
        );
        return;
      }
      case "modify": {
        const parsed = parseModifyCommand(args);
        if (typeof parsed === "string") {
          setFeedback({ kind: "error", message: parsed });
          return;
        }
        // Seuls les ordres en attente ont une structure vérifiée (CtraderOrder) — CtraderPosition
        // reste non vérifié (aucune position réelle observée), donc `modify` ne cible que les
        // ordres pour l'instant. Cf. commentaire équivalent dans ctrader/mappers.ts.
        const order = positions?.orders.find((o) => o.orderId === parsed.id);
        if (!order) {
          setFeedback({
            kind: "error",
            message: `ordre en attente ${parsed.id} introuvable`,
          });
          return;
        }
        setPendingModify({ order, stopLoss: parsed.stopLoss, takeProfit: parsed.takeProfit });
        setFeedback({ kind: "info", message: "modification calculée — confirme dans la popup" });
        return;
      }
      case "cancel": {
        const id = Number(args[0]);
        if (!Number.isFinite(id)) {
          setFeedback({
            kind: "error",
            message: `usage : cancel <id> — id invalide : "${args[0] ?? ""}"`,
          });
          return;
        }
        // Même limite que `modify` : seuls les ordres en attente (structure vérifiée) sont
        // annulables pour l'instant, pas les positions ouvertes (closePosition non exercé).
        const order = positions?.orders.find((o) => o.orderId === id);
        if (!order) {
          setFeedback({ kind: "error", message: `ordre en attente ${id} introuvable` });
          return;
        }
        setPendingCancel(order);
        setFeedback({ kind: "info", message: "annulation — confirme dans la popup" });
        return;
      }
      default:
        setFeedback({
          kind: "error",
          message: `commande inconnue : ${command} — help pour la liste`,
        });
    }
  }

  /** Cycle commun aux confirmations d'ordre : feedback "en cours" → action → feedback succès/erreur. */
  function runOrderAction(opts: {
    pending: string;
    action: () => Promise<unknown>;
    success: () => string;
    errorPrefix: string;
    refreshAfter?: boolean;
  }): void {
    setFeedback({ kind: "info", message: opts.pending });
    void opts.action().then(
      () => {
        setFeedback({ kind: "success", message: opts.success() });
        if (opts.refreshAfter) void refreshMarket();
      },
      (error) =>
        setFeedback({ kind: "error", message: `${opts.errorPrefix} : ${toMessage(error)}` }),
    );
  }

  function confirmPendingTrade() {
    if (!pendingTrade || !symbolId) return;
    const trade = pendingTrade;
    const summary = formatTradeSummary(trade);
    setPendingTrade(undefined);
    runOrderAction({
      pending: "envoi de l'ordre…",
      action: () => client.createOrder(toCreateOrderParams(symbolId, trade)),
      success: () => `ordre envoyé : ${summary}`,
      errorPrefix: "échec envoi",
    });
  }

  function cancelPendingTrade() {
    setPendingTrade(undefined);
    setFeedback({ kind: "info", message: "trade annulé" });
  }

  function confirmPendingModify() {
    if (!pendingModify) return;
    const { order, stopLoss, takeProfit } = pendingModify;
    setPendingModify(undefined);
    runOrderAction({
      pending: "modification en cours…",
      action: () => client.amendOrder({ orderId: order.orderId, stopLoss, takeProfit }),
      success: () => `ordre ${order.orderId} modifié`,
      errorPrefix: "échec modification",
      refreshAfter: true,
    });
  }

  function cancelPendingModify() {
    setPendingModify(undefined);
    setFeedback({ kind: "info", message: "modification annulée" });
  }

  function confirmPendingCancel() {
    if (!pendingCancel) return;
    const orderId = pendingCancel.orderId;
    setPendingCancel(undefined);
    runOrderAction({
      pending: "annulation en cours…",
      action: () => client.cancelOrder({ orderId }),
      success: () => `ordre ${orderId} annulé`,
      errorPrefix: "échec annulation",
      refreshAfter: true,
    });
  }

  function dismissPendingCancel() {
    setPendingCancel(undefined);
    setFeedback({ kind: "info", message: "annulation abandonnée" });
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
      <CommandBar
        ref={commandBarRef}
        feedback={feedback}
        onSubmit={runCommand}
        focused={!pendingTrade && !pendingModify && !pendingCancel}
      />
      {pendingTrade && (
        <TradeConfirmModal
          trade={pendingTrade}
          onConfirm={confirmPendingTrade}
          onCancel={cancelPendingTrade}
        />
      )}
      {pendingModify && (
        <ModifyConfirmModal
          order={pendingModify.order}
          stopLoss={pendingModify.stopLoss}
          takeProfit={pendingModify.takeProfit}
          onConfirm={confirmPendingModify}
          onCancel={cancelPendingModify}
        />
      )}
      {pendingCancel && (
        <CancelConfirmModal
          order={pendingCancel}
          onConfirm={confirmPendingCancel}
          onCancel={dismissPendingCancel}
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
