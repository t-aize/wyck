import { useCallback, useMemo, useRef, useState } from "react";
import { type AppConfig, readConfig } from "./config.ts";
import { SYMBOL } from "./constants.ts";
import { CtraderClient } from "./ctrader/client.ts";
import { computeOverallBias } from "./domain/smc/bias.ts";
import { CancelConfirmModal } from "./ui/components/CancelConfirmModal.tsx";
import { CommandBar, type CommandBarHandle } from "./ui/components/CommandBar.tsx";
import { MacroPanel } from "./ui/components/MacroPanel.tsx";
import { ModifyConfirmModal } from "./ui/components/ModifyConfirmModal.tsx";
import { NewsPanel } from "./ui/components/NewsPanel.tsx";
import { PositionsPanel } from "./ui/components/PositionsPanel.tsx";
import { PriceHeader } from "./ui/components/PriceHeader.tsx";
import { SetupScreen } from "./ui/components/SetupScreen.tsx";
import { StructurePanel } from "./ui/components/StructurePanel.tsx";
import { TradeConfirmModal } from "./ui/components/TradeConfirmModal.tsx";
import { useCalendar } from "./ui/hooks/useCalendar.ts";
import { useClock } from "./ui/hooks/useClock.ts";
import { useCtraderConnection } from "./ui/hooks/useCtraderConnection.ts";
import { useMacroData } from "./ui/hooks/useMacroData.ts";
import { useMarketData } from "./ui/hooks/useMarketData.ts";
import { useOrderActions } from "./ui/hooks/useOrderActions.ts";
import { useStructure } from "./ui/hooks/useStructure.ts";
import { useTerminalShortcuts } from "./ui/hooks/useTerminalShortcuts.ts";
import { theme } from "./ui/theme.ts";

/**
 * Porte d'entrée : pas de client MCP tant que la config (URL/token) n'est pas connue.
 * `key` sur ConnectedApp force un remount complet (nouveau client, hooks réinitialisés)
 * quand la commande `settings` fait passer par un nouveau round de SetupScreen.
 */
export function App() {
  const [config, setConfig] = useState<AppConfig | undefined>(() => readConfig());
  const [reconfiguring, setReconfiguring] = useState(false);
  // Compteur de générations plutôt que le secret lui-même : seul le fait que la config a
  // changé importe pour déclencher le remount, pas sa valeur.
  const [generation, setGeneration] = useState(0);

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
    <ConnectedApp key={generation} config={config} onReconfigure={() => setReconfiguring(true)} />
  );
}

function ConnectedApp({ config, onReconfigure }: { config: AppConfig; onReconfigure: () => void }) {
  const now = useClock();
  const [client] = useState(() => new CtraderClient(config));
  const commandBarRef = useRef<CommandBarHandle>(null);

  const { connected, symbolId, connectionError, setConnectionError } = useCtraderConnection(client);
  const { bid, ask, positions, balance, moneyDigits, refreshMarket } = useMarketData(
    client,
    symbolId,
    setConnectionError,
  );
  const { calendar, newsError, refreshNews } = useCalendar();
  const { rows: structureRows, structureError, refreshStructure } = useStructure(client, symbolId);
  // `config.fredApiKey` est maintenant obligatoire et ne peut changer que via un reconfig complet
  // (`settings` → remount via `generation`, cf. App()) — pas besoin d'état local séparé ici.
  const { macro, macroError, refreshMacro } = useMacroData(config.fredApiKey);
  const overallBias = useMemo(
    () => computeOverallBias(structureRows, macro, calendar, now),
    [structureRows, macro, calendar, now],
  );
  const {
    feedback,
    setFeedback,
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
  } = useOrderActions({
    client,
    symbolId,
    positions,
    refreshMarket,
    refreshNews,
    refreshStructure,
    refreshMacro,
    onReconfigure,
  });

  useTerminalShortcuts(
    setFeedback,
    useCallback(() => commandBarRef.current?.clearIfNotEmpty() ?? false, []),
  );

  return (
    <box
      style={{ flexDirection: "column", width: "100%", height: "100%", backgroundColor: theme.bg }}
    >
      <PriceHeader
        symbol={SYMBOL}
        bid={bid}
        ask={ask}
        connected={connected}
        now={now}
        errorMessage={connectionError}
        balance={balance}
        moneyDigits={moneyDigits}
      />
      <PositionsPanel positions={positions} now={now} bid={bid} ask={ask} />
      <MacroPanel macro={macro} errorMessage={macroError} />
      <box style={{ flexDirection: "row", flexGrow: 2, flexBasis: 0 }}>
        <NewsPanel events={calendar} errorMessage={newsError} now={now} />
        <StructurePanel rows={structureRows} errorMessage={structureError} bias={overallBias} />
      </box>
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
          orders={pendingCancel}
          onConfirm={confirmPendingCancel}
          onCancel={dismissPendingCancel}
        />
      )}
    </box>
  );
}
