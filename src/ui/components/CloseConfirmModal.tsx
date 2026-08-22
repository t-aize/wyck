import type { ClosablePosition } from "../../ctrader/schemas.ts";
import { computeUnrealizedPnlOrUndefined } from "../../trading/pnl.ts";
import { formatPriceOrDash } from "../format.ts";
import { pnlColor, sideColor } from "../theme.ts";
import { ConfirmModal, Row } from "./ConfirmModal.tsx";

interface CloseConfirmModalProps {
  position: ClosablePosition;
  /** Prix affiché (déjà divisé par PRICE_SCALE, cf. useMarketData). */
  bidPrice: number | undefined;
  askPrice: number | undefined;
  onConfirm: () => void;
  onCancel: () => void;
}

export function CloseConfirmModal({
  position,
  bidPrice,
  askPrice,
  onConfirm,
  onCancel,
}: CloseConfirmModalProps) {
  const pnl = computeUnrealizedPnlOrUndefined(
    position.side,
    position.volumeLots,
    position.entry,
    bidPrice,
    askPrice,
  );

  return (
    <ConfirmModal
      title="CLÔTURER LA POSITION"
      confirmLabel="✓ Confirmer — clôturer la position"
      cancelLabel="✗ Retour"
      onConfirm={onConfirm}
      onCancel={onCancel}
    >
      <Row label="Position" value={String(position.id)} />
      <Row
        label="Direction"
        value={`${position.side ?? "—"} ${position.volumeLots.toFixed(2)} lots`}
        fg={sideColor(position.side)}
      />
      <Row label="Entrée" value={formatPriceOrDash(position.entry)} />
      <Row label="P&L latent" value={pnl === undefined ? "—" : pnl.toFixed(2)} fg={pnlColor(pnl)} />
    </ConfirmModal>
  );
}
