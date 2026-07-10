import type { PreparedTrade } from "../domain/trading.ts";
import { ConfirmModal, Row } from "./ConfirmModal.tsx";
import { theme } from "./theme.ts";

interface TradeConfirmModalProps {
  trade: PreparedTrade;
  onConfirm: () => void;
  onCancel: () => void;
}

export function TradeConfirmModal({ trade, onConfirm, onCancel }: TradeConfirmModalProps) {
  const sideColor = trade.tradeSide === "BUY" ? theme.green : theme.red;

  return (
    <ConfirmModal
      title="CONFIRMER LE TRADE"
      confirmLabel="✓ Confirmer — envoyer l'ordre"
      onConfirm={onConfirm}
      onCancel={onCancel}
    >
      <Row label="Direction" value={`${trade.tradeSide} ${trade.orderType}`} fg={sideColor} />
      <Row label="Entrée" value={trade.entryPrice.toFixed(2)} />
      <Row label="Stop loss" value={trade.stopLoss.toFixed(2)} fg={theme.red} />
      <Row label="Take profit" value={trade.takeProfit.toFixed(2)} fg={theme.green} />
      <Row label="Volume" value={`${trade.volumeLots.toFixed(2)} lots`} />
      <Row
        label="Risque"
        value={`${trade.riskAmount.toFixed(2)} (${trade.riskPercent}%)`}
        fg={theme.red}
      />
      <Row label="Gain potentiel" value={trade.rewardAmount.toFixed(2)} fg={theme.green} />
    </ConfirmModal>
  );
}
