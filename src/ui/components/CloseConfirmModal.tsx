import { PRICE_SCALE } from "../../constants.ts";
import type { CtraderPosition } from "../../ctrader/schemas.ts";
import { computeUnrealizedPnl } from "../../domain/trading.ts";
import { formatPriceOrDash } from "../format.ts";
import { theme } from "../theme.ts";
import { ConfirmModal, Row } from "./ConfirmModal.tsx";

interface CloseConfirmModalProps {
  position: CtraderPosition & { id: number; volumeLots: number };
  /** Bid/ask bruts (échelle x10^5), comme reçus par PositionsPanel.tsx — convertis ici même. */
  bid: number | undefined;
  ask: number | undefined;
  onConfirm: () => void;
  onCancel: () => void;
}

export function CloseConfirmModal({
  position,
  bid,
  ask,
  onConfirm,
  onCancel,
}: CloseConfirmModalProps) {
  const bidPrice = bid === undefined ? undefined : bid / PRICE_SCALE;
  const askPrice = ask === undefined ? undefined : ask / PRICE_SCALE;
  const pnl =
    position.side !== undefined &&
    position.entry !== undefined &&
    bidPrice !== undefined &&
    askPrice !== undefined
      ? computeUnrealizedPnl(position.side, position.volumeLots, position.entry, bidPrice, askPrice)
      : undefined;

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
        fg={position.side === "SELL" ? theme.red : theme.green}
      />
      <Row label="Entrée" value={formatPriceOrDash(position.entry)} />
      <Row
        label="P&L latent"
        value={pnl === undefined ? "—" : pnl.toFixed(2)}
        fg={pnl === undefined ? theme.textDim : pnl >= 0 ? theme.green : theme.red}
      />
    </ConfirmModal>
  );
}
