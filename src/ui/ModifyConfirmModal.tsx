import type { CtraderOrder } from "../ctrader/client.ts";
import { ConfirmModal, Row } from "./ConfirmModal.tsx";
import { formatPriceOrDash } from "./format.ts";
import { theme } from "./theme.ts";

interface ModifyConfirmModalProps {
  order: CtraderOrder;
  stopLoss?: number;
  takeProfit?: number;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ModifyConfirmModal({
  order,
  stopLoss,
  takeProfit,
  onConfirm,
  onCancel,
}: ModifyConfirmModalProps) {
  return (
    <ConfirmModal
      title="MODIFIER L'ORDRE"
      confirmLabel="✓ Confirmer — modifier l'ordre"
      onConfirm={onConfirm}
      onCancel={onCancel}
    >
      <Row label="Ordre" value={`${order.orderId} ${order.tradeSide} ${order.orderType}`} />
      {stopLoss !== undefined && (
        <Row
          label="Stop loss"
          value={`${formatPriceOrDash(order.stopLoss)} → ${formatPriceOrDash(stopLoss)}`}
          fg={theme.red}
        />
      )}
      {takeProfit !== undefined && (
        <Row
          label="Take profit"
          value={`${formatPriceOrDash(order.takeProfit)} → ${formatPriceOrDash(takeProfit)}`}
          fg={theme.green}
        />
      )}
    </ConfirmModal>
  );
}
