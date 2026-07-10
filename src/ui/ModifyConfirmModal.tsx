import { useKeyboard } from "@opentui/react";
import type { CtraderOrder } from "../ctrader-client.ts";
import { theme } from "./theme.ts";

interface ModifyConfirmModalProps {
  order: CtraderOrder;
  stopLoss?: number;
  takeProfit?: number;
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

export function ModifyConfirmModal({
  order,
  stopLoss,
  takeProfit,
  onConfirm,
  onCancel,
}: ModifyConfirmModalProps) {
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
        title=" MODIFIER L'ORDRE "
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
        <Row label="Ordre" value={`${order.orderId} ${order.tradeSide} ${order.orderType}`} />
        {stopLoss !== undefined && (
          <Row
            label="Stop loss"
            value={`${fmt(order.stopLoss)} → ${fmt(stopLoss)}`}
            fg={theme.red}
          />
        )}
        {takeProfit !== undefined && (
          <Row
            label="Take profit"
            value={`${fmt(order.takeProfit)} → ${fmt(takeProfit)}`}
            fg={theme.green}
          />
        )}

        <box style={{ marginTop: 1 }}>
          <select
            focused
            options={[
              { name: "✓ Confirmer — modifier l'ordre", description: "" },
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
