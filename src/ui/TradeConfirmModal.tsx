import { useKeyboard } from "@opentui/react";
import type { PreparedTrade } from "../trading.ts";
import { theme } from "./theme.ts";

interface TradeConfirmModalProps {
  trade: PreparedTrade;
  onConfirm: () => void;
  onCancel: () => void;
}

function Row({ label, value, fg }: { label: string; value: string; fg?: string }) {
  return (
    <box style={{ flexDirection: "row", justifyContent: "space-between" }}>
      <text fg={theme.textDim}>{label}</text>
      <text fg={fg ?? theme.text}>{value}</text>
    </box>
  );
}

export function TradeConfirmModal({ trade, onConfirm, onCancel }: TradeConfirmModalProps) {
  const sideColor = trade.tradeSide === "BUY" ? theme.green : theme.red;

  // Échap annule, en plus de naviguer jusqu'à "Annuler" dans le <select>.
  useKeyboard((key) => {
    if (key.name === "escape") onCancel();
  });

  return (
    <box
      style={{
        position: "absolute",
        width: "100%",
        height: "100%",
        justifyContent: "center",
        alignItems: "center",
      }}
      zIndex={10}
    >
      <box
        title=" CONFIRMER LE TRADE "
        titleColor={theme.gold}
        style={{
          flexDirection: "column",
          width: 46,
          border: true,
          borderColor: theme.gold,
          backgroundColor: theme.panelBg,
          paddingLeft: 2,
          paddingRight: 2,
          paddingTop: 1,
          paddingBottom: 1,
          rowGap: 0,
        }}
      >
        <Row label="Direction" value={`${trade.tradeSide} ${trade.orderType}`} fg={sideColor} />
        <Row label="Entrée" value={trade.entryPrice.toFixed(2)} />
        <Row label="Stop loss" value={trade.stopLoss.toFixed(2)} fg={theme.red} />
        <Row label="Take profit" value={trade.takeProfit.toFixed(2)} fg={theme.green} />
        <Row label="Volume" value={`${trade.volumeLots.toFixed(2)} lots`} />
        <Row
          label="Risque"
          value={`${trade.riskAmount.toFixed(2)} (${trade.riskPercent}%)`}
          fg={theme.red}
        />
        <Row label="Gain potentiel" value={trade.rewardAmount.toFixed(2)} fg={theme.green} />

        <box style={{ marginTop: 1 }}>
          <select
            focused
            options={[
              { name: "✓ Confirmer — envoyer l'ordre", description: "" },
              { name: "✗ Annuler", description: "" },
            ]}
            showDescription={false}
            style={{ height: 2 }}
            selectedBackgroundColor={theme.gold}
            selectedTextColor={theme.bg}
            onSelect={(index) => (index === 0 ? onConfirm() : onCancel())}
          />
        </box>

        <text fg={theme.textMuted}>↑↓ naviguer · Entrée valider · Échap annuler</text>
      </box>
    </box>
  );
}
