import { useCallback, useEffect, useRef, useState } from "react";
import { readConfig } from "./config.ts";
import { SYMBOL } from "./constants.ts";
import type { CtraderClientConfig } from "./ctrader/client.ts";
import { AmendConfirmModal } from "./ui/components/AmendConfirmModal.tsx";
import { CancelConfirmModal } from "./ui/components/CancelConfirmModal.tsx";
import { CloseConfirmModal } from "./ui/components/CloseConfirmModal.tsx";
import { CommandBar, type CommandBarHandle } from "./ui/components/CommandBar.tsx";
import { Row } from "./ui/components/ConfirmModal.tsx";
import { NewsPanel } from "./ui/components/NewsPanel.tsx";
import { PositionsPanel } from "./ui/components/PositionsPanel.tsx";
import { PriceHeader } from "./ui/components/PriceHeader.tsx";
import { SetupScreen } from "./ui/components/SetupScreen.tsx";
import { TradeConfirmModal } from "./ui/components/TradeConfirmModal.tsx";
import { CtraderProvider, useCtrader } from "./ui/context/CtraderContext.tsx";
import { FeedbackProvider, useFeedback } from "./ui/context/FeedbackContext.tsx";
import { useCalendar } from "./ui/hooks/useCalendar.ts";
import { useClock } from "./ui/hooks/useClock.ts";
import { useMarketData } from "./ui/hooks/useMarketData.ts";
import { useOrderActions } from "./ui/hooks/useOrderActions.ts";
import { useTerminalShortcuts } from "./ui/hooks/useTerminalShortcuts.ts";
import { theme } from "./ui/theme.ts";
import { fsRuntime } from "./utils/effectRuntime.ts";

/**
 * Porte d'entrée : pas de client MCP tant que la config (URL/token) n'est pas connue.
 * `key` sur ConnectedApp force un remount complet (nouveau client, hooks réinitialisés)
 * quand la commande `settings` fait passer par un nouveau round de SetupScreen.
 */
export function App() {
  // `null` = pas encore chargée (distinct de `undefined` = chargée, aucune config trouvée).
  // FileSystem (@effect/platform-bun) fait de l'I/O réellement async (contrairement aux
  // readFileSync/existsSync d'avant) — impossible à résoudre avec Effect.runSync dans
  // l'initializer synchrone de useState (AsyncFiberException à l'exécution, vérifié en
  // pratique) : il faut vraiment attendre le premier rendu.
  const [config, setConfig] = useState<CtraderClientConfig | undefined | null>(null);
  const [reconfiguring, setReconfiguring] = useState(false);
  // Compteur de générations plutôt que le secret lui-même : seul le fait que la config a
  // changé importe pour déclencher le remount, pas sa valeur.
  const [generation, setGeneration] = useState(0);

  useEffect(() => {
    void fsRuntime.runPromise(readConfig()).then(setConfig);
  }, []);

  if (config === null) {
    return (
      <box
        style={{
          flexDirection: "column",
          width: "100%",
          height: "100%",
          backgroundColor: theme.bg,
          justifyContent: "center",
          alignItems: "center",
        }}
      >
        <text fg={theme.textDim}>chargement…</text>
      </box>
    );
  }

  if (!config || reconfiguring) {
    return (
      <SetupScreen
        initial={config}
        onConfigured={(next) => {
          setConfig(next);
          setReconfiguring(false);
          setGeneration((g) => g + 1);
        }}
        onCancel={config ? () => setReconfiguring(false) : undefined}
      />
    );
  }

  return (
    <CtraderProvider key={generation} config={config}>
      <FeedbackProvider>
        <ConnectedApp onReconfigure={() => setReconfiguring(true)} />
      </FeedbackProvider>
    </CtraderProvider>
  );
}

/** `connected`/`connectionError` viennent de `useCtrader()`, `feedback` de `useFeedback()` —
 * `client`/`symbolId`/`setFeedback` ne sont plus lus ici : useOrderActions.ts et les hooks issus de
 * son éclatement les lisent eux-mêmes via ces Contexts. */
function ConnectedApp({ onReconfigure }: { onReconfigure: () => void }) {
  const now = useClock();
  const { connected, connectionError } = useCtrader();
  const { feedback } = useFeedback();
  const commandBarRef = useRef<CommandBarHandle>(null);

  const {
    bid,
    ask,
    bidPrice,
    askPrice,
    priceHistory,
    positions,
    balance,
    moneyDigits,
    refreshMarket,
  } = useMarketData();
  const { calendar, newsError, refreshNews } = useCalendar();

  const {
    runCommand,
    pendingTrade,
    confirmPendingTrade,
    cancelPendingTrade,
    pendingModify,
    confirmPendingModify,
    cancelPendingModify,
    pendingCancel,
    confirmPendingCancel,
    dismissPendingCancel,
    pendingPositionAmend,
    confirmPendingPositionAmend,
    cancelPendingPositionAmend,
    pendingClose,
    confirmPendingClose,
    dismissPendingClose,
  } = useOrderActions({ positions, refreshMarket, refreshNews, onReconfigure });

  useTerminalShortcuts(useCallback(() => commandBarRef.current?.clearIfNotEmpty() ?? false, []));

  return (
    <box
      style={{ flexDirection: "column", width: "100%", height: "100%", backgroundColor: theme.bg }}
    >
      <PriceHeader
        symbol={SYMBOL}
        bid={bid}
        ask={ask}
        priceHistory={priceHistory}
        connected={connected}
        now={now}
        errorMessage={connectionError}
        balance={balance}
        moneyDigits={moneyDigits}
      />
      <PositionsPanel positions={positions} bidPrice={bidPrice} askPrice={askPrice} />
      <NewsPanel events={calendar} errorMessage={newsError} now={now} />
      <CommandBar
        ref={commandBarRef}
        feedback={feedback}
        onSubmit={runCommand}
        focused={
          !pendingTrade &&
          !pendingModify &&
          !pendingCancel &&
          !pendingPositionAmend &&
          !pendingClose
        }
      />
      {pendingTrade && (
        <TradeConfirmModal
          trade={pendingTrade}
          onConfirm={confirmPendingTrade}
          onCancel={cancelPendingTrade}
        />
      )}
      {pendingModify && (
        <AmendConfirmModal
          title="MODIFIER L'ORDRE"
          confirmLabel="✓ Confirmer — modifier l'ordre"
          subjectRow={
            <Row
              label="Ordre"
              value={`${pendingModify.order.orderId} ${pendingModify.order.tradeSide} ${pendingModify.order.orderType}`}
            />
          }
          currentStopLoss={pendingModify.order.stopLoss}
          currentTakeProfit={pendingModify.order.takeProfit}
          nextStopLoss={pendingModify.stopLoss}
          nextTakeProfit={pendingModify.takeProfit}
          onConfirm={confirmPendingModify}
          onCancel={cancelPendingModify}
        />
      )}
      {pendingCancel && (
        <CancelConfirmModal
          orders={pendingCancel}
          onConfirm={confirmPendingCancel}
          onCancel={dismissPendingCancel}
        />
      )}
      {pendingPositionAmend && (
        <AmendConfirmModal
          title="MODIFIER LA POSITION"
          confirmLabel="✓ Confirmer — modifier la position"
          subjectRow={
            <Row
              label="Position"
              value={`${pendingPositionAmend.position.id} ${pendingPositionAmend.position.side ?? "—"} ${pendingPositionAmend.position.volumeLots?.toFixed(2) ?? "—"} lots`}
            />
          }
          currentStopLoss={pendingPositionAmend.position.stopLoss}
          currentTakeProfit={pendingPositionAmend.position.takeProfit}
          nextStopLoss={pendingPositionAmend.stopLoss}
          nextTakeProfit={pendingPositionAmend.takeProfit}
          onConfirm={confirmPendingPositionAmend}
          onCancel={cancelPendingPositionAmend}
        />
      )}
      {pendingClose && (
        <CloseConfirmModal
          position={pendingClose}
          bidPrice={bidPrice}
          askPrice={askPrice}
          onConfirm={confirmPendingClose}
          onCancel={dismissPendingClose}
        />
      )}
    </box>
  );
}
