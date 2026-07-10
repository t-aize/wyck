import { toLots } from "../../constants.ts";
import type { CtraderOrder } from "../../ctrader/client.ts";
import { formatPriceOrDash } from "../format.ts";
import { theme } from "../theme.ts";
import { ConfirmModal, Row } from "./ConfirmModal.tsx";

interface CancelConfirmModalProps {
  order: CtraderOrder;
  onConfirm: () => void;
  onCancel: () => void;
}

export function CancelConfirmModal({ order, onConfirm, onCancel }: CancelConfirmModalProps) {
  const sideColor = order.tradeSide === "SELL" ? theme.red : theme.green;

  return (
    <ConfirmModal
      title="ANNULER L'ORDRE"
      confirmLabel="✓ Confirmer — annuler l'ordre"
      cancelLabel="✗ Retour"
      onConfirm={onConfirm}
      onCancel={onCancel}
    >
      <Row label="Ordre" value={String(order.orderId)} />
      <Row label="Direction" value={`${order.tradeSide} ${order.orderType}`} fg={sideColor} />
      <Row label="Volume" value={`${toLots(order.volume).toFixed(2)} lots`} />
      <Row label="Prix" value={formatPriceOrDash(order.limitPrice ?? order.stopPrice)} />
      <Row label="Stop loss" value={formatPriceOrDash(order.stopLoss)} fg={theme.red} />
      <Row label="Take profit" value={formatPriceOrDash(order.takeProfit)} fg={theme.green} />
    </ConfirmModal>
  );
}
