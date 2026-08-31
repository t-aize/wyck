import { useCallback, useEffect, useRef, useState } from "react";
import { SYMBOL, type TrendbarPeriod } from "./constants.ts";
import { type AppConfig, EMPTY_APP_CONFIG, readConfig } from "./settings.ts";
import { AmendConfirmModal } from "./ui/components/AmendConfirmModal.tsx";
import { CancelConfirmModal } from "./ui/components/CancelConfirmModal.tsx";
import { CloseConfirmModal } from "./ui/components/CloseConfirmModal.tsx";
import { CommandBar, type CommandBarHandle } from "./ui/components/CommandBar.tsx";
import { Row } from "./ui/components/ConfirmModal.tsx";
import { NewsPanel } from "./ui/components/NewsPanel.tsx";
import { PositionsPanel } from "./ui/components/PositionsPanel.tsx";
import { PriceHeader } from "./ui/components/PriceHeader.tsx";
import { StructureBar } from "./ui/components/StructureBar.tsx";
import { TradeConfirmModal } from "./ui/components/TradeConfirmModal.tsx";
import { CtraderProvider, useCtrader } from "./ui/context/CtraderContext.tsx";
import { FeedbackProvider, useFeedback } from "./ui/context/FeedbackContext.tsx";
import {
  atrRefreshMs,
  nextAtrBoundaryMs,
  useAtrAutoRefresh,
} from "./ui/hooks/useAtrAutoRefresh.ts";
import { useCalendar } from "./ui/hooks/useCalendar.ts";
import { useClock } from "./ui/hooks/useClock.ts";
import { useMarketData } from "./ui/hooks/useMarketData.ts";
import { useOrderActions } from "./ui/hooks/useOrderActions.ts";
import { useStructure } from "./ui/hooks/useStructure.ts";
import { useTerminalShortcuts } from "./ui/hooks/useTerminalShortcuts.ts";
import { theme } from "./ui/theme.ts";
import { fsRuntime } from "./utils/effectRuntime.ts";

/**
 * Porte d'entrée : rend toujours `ConnectedApp` dès que la lecture disque initiale est terminée —
 * configuré ou non. Sans url/token, `CtraderClient#isConfigured` est `false` (cf.
 * useCtraderConnection.ts) : l'app reste dans son état "pas encore connecté" neutre (header en
 * CONNEXION…, positions/prix vides) jusqu'à ce que `settings url`/`settings token` (cf.
 * commands/settings.ts) persistent des identifiants et déclenchent `reloadConfig` ci-dessous — plus
 * d'assistant de configuration séparé à bloquer dessus.
 * `key` sur CtraderProvider force un remount complet (nouveau client, hooks réinitialisés) à chaque
 * appel de `reloadConfig`.
 */
export function App() {
  // `null` = pas encore chargée (distinct de `undefined` = chargée, aucun fichier trouvé).
  // FileSystem (@effect/platform-bun) fait de l'I/O réellement async (contrairement aux
  // readFileSync/existsSync d'avant) — impossible à résoudre avec Effect.runSync dans
  // l'initializer synchrone de useState (AsyncFiberException à l'exécution, vérifié en
  // pratique) : il faut vraiment attendre le premier rendu.
  const [config, setConfig] = useState<AppConfig | undefined | null>(null);
  // Compteur de générations plutôt que le secret lui-même : seul le fait que la config a
  // changé importe pour déclencher le remount, pas sa valeur.
  const [generation, setGeneration] = useState(0);

  useEffect(() => {
    void fsRuntime.runPromise(readConfig()).then(setConfig);
  }, []);

  // Appelé par `settings url`/`settings token` (cf. useCommandRouter.ts) une fois l'écriture sur
  // disque terminée : relit le fichier puis force le remount ci-dessous, qui reconstruit le
  // CtraderClient avec les nouveaux identifiants et retente une connexion.
  const reloadConfig = useCallback(() => {
    void fsRuntime.runPromise(readConfig()).then((next) => {
      setConfig(next);
      setGeneration((g) => g + 1);
    });
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

  const effectiveConfig = config ?? EMPTY_APP_CONFIG;

  return (
    // FeedbackProvider au-dessus de CtraderProvider (pas dedans) : un remount déclenché par
    // reloadConfig démonte tout ce qui est sous CtraderProvider, feedback compris si imbriqué — en
    // le sortant, le message de confirmation de `settings url`/`settings token` reste affiché
    // pendant la reconnexion au lieu de disparaître aussitôt.
    <FeedbackProvider>
      <CtraderProvider key={generation} config={effectiveConfig}>
        <ConnectedApp
          onCredentialsChanged={reloadConfig}
          hasMcpUrl={effectiveConfig.url.trim() !== ""}
          hasMcpToken={effectiveConfig.token.trim() !== ""}
          initialAtrRefreshEnabled={effectiveConfig.atrRefreshEnabled}
          initialAtrPeriod={effectiveConfig.atrPeriod}
          initialAtrTimeframe={effectiveConfig.atrTimeframe}
        />
      </CtraderProvider>
    </FeedbackProvider>
  );
}

/** `connected`/`connectionError` viennent de `useCtrader()`, `feedback` de `useFeedback()` —
 * `client`/`symbolId`/`setFeedback` ne sont plus lus ici : useOrderActions.ts et les hooks issus de
 * son éclatement les lisent eux-mêmes via ces Contexts. */
function ConnectedApp({
  onCredentialsChanged,
  hasMcpUrl,
  hasMcpToken,
  initialAtrRefreshEnabled,
  initialAtrPeriod,
  initialAtrTimeframe,
}: {
  onCredentialsChanged: () => void;
  hasMcpUrl: boolean;
  hasMcpToken: boolean;
  initialAtrRefreshEnabled: boolean;
  initialAtrPeriod: number;
  initialAtrTimeframe: TrendbarPeriod;
}) {
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
    spreadHistory,
    positions,
    balance,
    moneyDigits,
    refreshMarket,
  } = useMarketData();
  const { calendar, newsError, refreshNews } = useCalendar();
  const { structure, structureError } = useStructure();

  const {
    runCommand,
    atrMode,
    toggleAtrMode,
    atrRefreshEnabled,
    atrPeriod,
    atrTimeframe,
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
  } = useOrderActions({
    positions,
    refreshMarket,
    refreshNews,
    onCredentialsChanged,
    hasMcpUrl,
    hasMcpToken,
    initialAtrRefreshEnabled,
    initialAtrPeriod,
    initialAtrTimeframe,
  });

  useTerminalShortcuts(
    useCallback(() => commandBarRef.current?.clearIfNotEmpty() ?? false, []),
    toggleAtrMode,
    atrMode,
  );

  const { trackedOrderIds: atrOrderIds } = useAtrAutoRefresh({
    positions,
    enabled: atrRefreshEnabled,
    refreshMarket,
    atrPeriod,
    atrTimeframe,
  });
  // Aligné sur la vraie clôture de bougie du timeframe ATR configuré (cf. nextAtrBoundaryMs), pas
  // sur l'instant de lancement de l'app — fermer/rouvrir le terminal ne fait donc pas repartir le
  // compte à rebours à l'intervalle plein.
  const atrRefreshSecondsRemaining = Math.max(
    0,
    Math.ceil(
      (nextAtrBoundaryMs(now.getTime(), atrRefreshMs(atrTimeframe)) - now.getTime()) / 1000,
    ),
  );

  return (
    <box
      style={{ flexDirection: "column", width: "100%", height: "100%", backgroundColor: theme.bg }}
    >
      <PriceHeader
        symbol={SYMBOL}
        bid={bid}
        ask={ask}
        priceHistory={priceHistory}
        spreadHistory={spreadHistory}
        connected={connected}
        now={now}
        errorMessage={connectionError}
        balance={balance}
        moneyDigits={moneyDigits}
        configured={hasMcpUrl && hasMcpToken}
      />
      <StructureBar structure={structure} errorMessage={structureError} />
      <PositionsPanel
        positions={positions}
        bidPrice={bidPrice}
        askPrice={askPrice}
        atrOrderIds={atrOrderIds}
      />
      <NewsPanel events={calendar} errorMessage={newsError} now={now} />
      <CommandBar
        ref={commandBarRef}
        feedback={feedback}
        atrMode={atrMode}
        atrPeriod={atrPeriod}
        atrTimeframe={atrTimeframe}
        atrRefreshEnabled={atrRefreshEnabled}
        atrRefreshSecondsRemaining={atrRefreshSecondsRemaining}
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
