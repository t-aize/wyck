import type { CtraderOrder } from "../../ctrader/book/CtraderOrder.ts";
import { toLots } from "../../utils/priceMath.ts";
import { useCtrader } from "../context/CtraderContext.tsx";
import { formatPriceOrDash } from "../format.ts";
import { sideColor, theme } from "../theme.ts";
import { ConfirmModal, Row } from "./ConfirmModal.tsx";

interface CancelConfirmModalProps {
  orders: CtraderOrder[];
  onConfirm: () => void;
  onCancel: () => void;
}

export function CancelConfirmModal({ orders, onConfirm, onCancel }: CancelConfirmModalProps) {
  const { catalog } = useCtrader();
  const catalogById = new Map(catalog.map((item) => [item.symbolId, item]));
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
        const specs = catalogById.get(order.symbolId);
        const digits = specs?.digits ?? 2;
        const lots = specs ? toLots(order.volume, specs.lotSize) : undefined;
        return (
          <box key={order.orderId} flexDirection="column" marginTop={i > 0 ? 1 : 0} rowGap={0}>
            <Row label="Ordre" value={`${order.orderId}${specs ? ` ${specs.symbolName}` : ""}`} />
            <Row
              label="Direction"
              value={`${order.tradeSide} ${order.orderType}`}
              fg={sideColor(order.tradeSide)}
            />
            <Row label="Volume" value={`${lots?.toFixed(2) ?? "—"} lots`} />
            <Row
              label="Prix"
              value={formatPriceOrDash(order.limitPrice ?? order.stopPrice, digits)}
            />
            <Row
              label="Stop loss"
              value={formatPriceOrDash(order.stopLoss, digits)}
              fg={theme.red}
            />
            <Row
              label="Take profit"
              value={formatPriceOrDash(order.takeProfit, digits)}
              fg={theme.green}
            />
          </box>
        );
      })}
    </ConfirmModal>
  );
}
