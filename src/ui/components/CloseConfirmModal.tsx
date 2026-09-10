import type { ClosablePosition } from "../../ctrader/schemas.ts";
import type { InstrumentSpecs } from "../../instrument/specs.ts";
import { computeUnrealizedPnlOrUndefined } from "../../trading/pnl.ts";
import { toLots } from "../../utils/priceMath.ts";
import { formatPriceOrDash } from "../format.ts";
import { pnlColor, sideColor } from "../theme.ts";
import { ConfirmModal, Row } from "./ConfirmModal.tsx";

interface CloseConfirmModalProps {
  position: ClosablePosition;
  specs: InstrumentSpecs | undefined;
  bidPrice: number | undefined;
  askPrice: number | undefined;
  onConfirm: () => void;
  onCancel: () => void;
}

export function CloseConfirmModal({
  position,
  specs,
  bidPrice,
  askPrice,
  onConfirm,
  onCancel,
}: CloseConfirmModalProps) {
  const pnl = computeUnrealizedPnlOrUndefined(
    position.side,
    position.volume,
    position.entry,
    bidPrice,
    askPrice,
  );
  const lots = specs === undefined ? undefined : toLots(position.volume, specs.lotSize);
  const digits = specs?.digits ?? 2;

  return (
    <ConfirmModal
      title="CLÔTURER LA POSITION"
      confirmLabel="✓ Confirmer — clôturer la position"
      cancelLabel="✗ Retour"
      onConfirm={onConfirm}
      onCancel={onCancel}
    >
      <Row label="Position" value={`${position.id}${specs ? ` ${specs.symbolName}` : ""}`} />
      <Row
        label="Direction"
        value={`${position.side ?? "—"} ${lots?.toFixed(2) ?? "—"} lots`}
        fg={sideColor(position.side)}
      />
      <Row label="Entrée" value={formatPriceOrDash(position.entry, digits)} />
      <Row label="P&L latent" value={pnl === undefined ? "—" : pnl.toFixed(2)} fg={pnlColor(pnl)} />
    </ConfirmModal>
  );
}
