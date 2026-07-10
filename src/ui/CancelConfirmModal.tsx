import { useKeyboard } from "@opentui/react";
import type { CtraderOrder } from "../ctrader-client.ts";
import { theme } from "./theme.ts";

interface CancelConfirmModalProps {
  order: CtraderOrder;
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

function fmt(price: number | undefined): string {
  return price === undefined ? "—" : price.toFixed(2);
}

export function CancelConfirmModal({ order, onConfirm, onCancel }: CancelConfirmModalProps) {
  const sideColor = order.tradeSide === "SELL" ? theme.red : theme.green;

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
        title=" ANNULER L'ORDRE "
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
        <Row label="Ordre" value={String(order.orderId)} />
        <Row label="Direction" value={`${order.tradeSide} ${order.orderType}`} fg={sideColor} />
        <Row label="Volume" value={`${(order.volume / 10_000).toFixed(2)} lots`} />
        <Row label="Prix" value={fmt(order.limitPrice ?? order.stopPrice)} />
        <Row label="Stop loss" value={fmt(order.stopLoss)} fg={theme.red} />
        <Row label="Take profit" value={fmt(order.takeProfit)} fg={theme.green} />

        <box style={{ marginTop: 1 }}>
          <select
            focused
            options={[
              { name: "✓ Confirmer — annuler l'ordre", description: "" },
              { name: "✗ Retour", description: "" },
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
