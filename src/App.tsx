import { useCallback, useEffect, useRef, useState } from "react";
import { type AppConfig, type AtrSettings, readConfig, writeConfig } from "./config.ts";
import { SYMBOL } from "./constants.ts";
import { fsRuntime } from "./effectRuntime.ts";
import { CancelConfirmModal } from "./ui/components/CancelConfirmModal.tsx";
import { CommandBar, type CommandBarHandle } from "./ui/components/CommandBar.tsx";
import { ModifyConfirmModal } from "./ui/components/ModifyConfirmModal.tsx";
import { NewsPanel } from "./ui/components/NewsPanel.tsx";
import { PositionsPanel } from "./ui/components/PositionsPanel.tsx";
import { PriceHeader } from "./ui/components/PriceHeader.tsx";
import { SetupScreen } from "./ui/components/SetupScreen.tsx";
import { TradeConfirmModal } from "./ui/components/TradeConfirmModal.tsx";
import { TrendPanel } from "./ui/components/TrendPanel.tsx";
import { CtraderProvider, useCtrader } from "./ui/context/CtraderContext.tsx";
import { FeedbackProvider, useFeedback } from "./ui/context/FeedbackContext.tsx";
import { useAtrOrderTracking } from "./ui/hooks/useAtrOrderTracking.ts";
import { useCalendar } from "./ui/hooks/useCalendar.ts";
import { useClock } from "./ui/hooks/useClock.ts";
import { useMarketData } from "./ui/hooks/useMarketData.ts";
import { useOrderActions } from "./ui/hooks/useOrderActions.ts";
import { useTerminalShortcuts } from "./ui/hooks/useTerminalShortcuts.ts";
import { useTrend } from "./ui/hooks/useTrend.ts";
import { theme } from "./ui/theme.ts";

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
  const [config, setConfig] = useState<AppConfig | undefined | null>(null);
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

  // Persiste (config.json) + met à jour l'état local en une passe — `setConfig` reçoit une
  // fonction plutôt que `{ ...config, ...patch }` construit en dehors : si `config` avait déjà
  // changé entretemps (peu probable ici vu la source unique de mise à jour, mais cohérent avec le
  // reste du fichier qui préfère les mises à jour fonctionnelles), on part toujours de la valeur
  // la plus fraîche.
  function updateAtrSettings(patch: Partial<AtrSettings>) {
    setConfig((current) => {
      if (!current) return current;
      const next = { ...current, ...patch };
      void fsRuntime.runPromise(writeConfig(next));
      return next;
    });
  }

  return (
    <CtraderProvider key={generation} config={config}>
      <FeedbackProvider>
        <ConnectedApp
          config={config}
          onReconfigure={() => setReconfiguring(true)}
          onUpdateAtrSettings={updateAtrSettings}
        />
      </FeedbackProvider>
    </CtraderProvider>
  );
}

/** `client`/`runtime`/`connected`/`symbolId`/`connectionError` viennent de `useCtrader()`,
 * `feedback`/`setFeedback` de `useFeedback()` (cf. docs/ARCHITECTURE.md §8) — plus besoin de les
 * construire ni de les enfiler à la main à travers ce composant. */
function ConnectedApp({
  config,
  onReconfigure,
  onUpdateAtrSettings,
}: {
  config: AppConfig;
  onReconfigure: () => void;
  onUpdateAtrSettings: (patch: Partial<AtrSettings>) => void;
}) {
  const now = useClock();
  const { client, runtime, connected, symbolId, connectionError } = useCtrader();
  const { feedback, setFeedback } = useFeedback();
  const commandBarRef = useRef<CommandBarHandle>(null);

  const { bid, ask, priceHistory, positions, balance, moneyDigits, refreshMarket } =
    useMarketData();
  const { calendar, newsError, refreshNews } = useCalendar();
  const {
    rows: trendRows,
    atr: atrRaw,
    trendError,
    refreshTrend,
  } = useTrend(config.atrPeriod, config.atrTimeframe);

  const atrSettings: AtrSettings = {
    rewardRiskRatio: config.rewardRiskRatio,
    atrMultiplier: config.atrMultiplier,
    atrPeriod: config.atrPeriod,
    atrTimeframe: config.atrTimeframe,
  };

  const { trackedOrderIds, registerPendingAtrOrder, untrackOrder } = useAtrOrderTracking({
    pendingOrders: positions?.orders ?? [],
    atrRaw,
  });

  const {
    runCommand,
    atrMode,
    toggleAtrMode,
    pendingTrade,
    confirmPendingTrade,
    cancelPendingTrade,
    pendingModify,
    confirmPendingModify,
    cancelPendingModify,
    pendingCancel,
    confirmPendingCancel,
    dismissPendingCancel,
  } = useOrderActions({
    setFeedback,
    client,
    runtime,
    symbolId,
    positions,
    refreshMarket,
    refreshNews,
    refreshTrend,
    trendRows,
    onReconfigure,
    atrRaw,
    atrSettings,
    onUpdateAtrSettings,
    atrTracking: { registerPendingAtrOrder, untrackOrder },
  });

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
      <PositionsPanel
        positions={positions}
        now={now}
        bid={bid}
        ask={ask}
        trackedOrderIds={trackedOrderIds}
      />
      <box style={{ flexDirection: "row", flexGrow: 2, flexBasis: 0 }}>
        <NewsPanel events={calendar} errorMessage={newsError} now={now} />
        <TrendPanel rows={trendRows} errorMessage={trendError} />
      </box>
      <CommandBar
        ref={commandBarRef}
        feedback={feedback}
        onSubmit={runCommand}
        focused={!pendingTrade && !pendingModify && !pendingCancel}
        atrMode={atrMode}
        onToggleAtrMode={toggleAtrMode}
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
          orders={pendingCancel}
          onConfirm={confirmPendingCancel}
          onCancel={dismissPendingCancel}
        />
      )}
    </box>
  );
}
