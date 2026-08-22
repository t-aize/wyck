import type { CtraderPosition } from "../../ctrader/schemas.ts";
import { formatPriceOrDash } from "../format.ts";
import { theme } from "../theme.ts";
import { ConfirmModal, Row } from "./ConfirmModal.tsx";

interface PositionAmendConfirmModalProps {
  position: CtraderPosition & { id: number };
  stopLoss?: number;
  takeProfit?: number;
  onConfirm: () => void;
  onCancel: () => void;
}

export function PositionAmendConfirmModal({
  position,
  stopLoss,
  takeProfit,
  onConfirm,
  onCancel,
}: PositionAmendConfirmModalProps) {
  return (
    <ConfirmModal
      title="MODIFIER LA POSITION"
      confirmLabel="✓ Confirmer — modifier la position"
      onConfirm={onConfirm}
      onCancel={onCancel}
    >
      <Row
        label="Position"
        value={`${position.id} ${position.side ?? "—"} ${position.volumeLots?.toFixed(2) ?? "—"} lots`}
      />
      {stopLoss !== undefined && (
        <Row
          label="Stop loss"
          value={`${formatPriceOrDash(position.stopLoss)} → ${formatPriceOrDash(stopLoss)}`}
          fg={theme.red}
        />
      )}
      {takeProfit !== undefined && (
        <Row
          label="Take profit"
          value={`${formatPriceOrDash(position.takeProfit)} → ${formatPriceOrDash(takeProfit)}`}
          fg={theme.green}
        />
      )}
    </ConfirmModal>
  );
}
