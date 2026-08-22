import type { CtraderOrder } from "../../ctrader/schemas.ts";
import { toLots } from "../../utils/priceMath.ts";
import { formatPriceOrDash } from "../format.ts";
import { sideColor, theme } from "../theme.ts";
import { ConfirmModal, Row } from "./ConfirmModal.tsx";

interface CancelConfirmModalProps {
  orders: CtraderOrder[];
  onConfirm: () => void;
  onCancel: () => void;
}

export function CancelConfirmModal({ orders, onConfirm, onCancel }: CancelConfirmModalProps) {
  const title = orders.length > 1 ? `ANNULER ${orders.length} ORDRES` : "ANNULER L'ORDRE";
  const confirmLabel =
    orders.length > 1
      ? `✓ Confirmer — annuler ${orders.length} ordres`
      : "✓ Confirmer — annuler l'ordre";

  return (
    <ConfirmModal
      title={title}
      confirmLabel={confirmLabel}
      cancelLabel="✗ Retour"
      onConfirm={onConfirm}
      onCancel={onCancel}
    >
      {orders.map((order, i) => {
        return (
          <box
            key={order.orderId}
            style={{ flexDirection: "column", marginTop: i > 0 ? 1 : 0, rowGap: 0 }}
          >
            <Row label="Ordre" value={String(order.orderId)} />
            <Row
              label="Direction"
              value={`${order.tradeSide} ${order.orderType}`}
              fg={sideColor(order.tradeSide)}
            />
            <Row label="Volume" value={`${toLots(order.volume).toFixed(2)} lots`} />
            <Row label="Prix" value={formatPriceOrDash(order.limitPrice ?? order.stopPrice)} />
            <Row label="Stop loss" value={formatPriceOrDash(order.stopLoss)} fg={theme.red} />
            <Row label="Take profit" value={formatPriceOrDash(order.takeProfit)} fg={theme.green} />
          </box>
        );
      })}
    </ConfirmModal>
  );
}
