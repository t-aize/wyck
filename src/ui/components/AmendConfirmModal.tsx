import type { ReactNode } from "react";
import { formatPriceOrDash } from "../format.ts";
import { theme } from "../theme.ts";
import { ConfirmModal, Row } from "./ConfirmModal.tsx";

interface AmendConfirmModalProps {
  title: string;
  confirmLabel: string;
  /** Ligne résumant la cible de l'amend (ordre ou position) — le contenu diffère réellement selon
   * l'entité, laissé à l'appelant plutôt que de forcer un format commun. */
  subjectRow: ReactNode;
  currentStopLoss: number | undefined;
  currentTakeProfit: number | undefined;
  nextStopLoss?: number;
  nextTakeProfit?: number;
  onConfirm: () => void;
  onCancel: () => void;
}

/** Modale d'amend partagée par les commandes "modify order" et "modify position" — cTrader traite
 * déjà les deux comme la même intention (`amend_order`/`amend_position`, cf. commands/amend.ts) ;
 * les deux modales d'origine (ModifyConfirmModal/PositionAmendConfirmModal) n'étaient distinguées
 * que par leur ligne "sujet" et leur titre, ce que `subjectRow`/`title`/`confirmLabel` couvrent ici. */
export function AmendConfirmModal({
  title,
  confirmLabel,
  subjectRow,
  currentStopLoss,
  currentTakeProfit,
  nextStopLoss,
  nextTakeProfit,
  onConfirm,
  onCancel,
}: AmendConfirmModalProps) {
  return (
    <ConfirmModal
      title={title}
      confirmLabel={confirmLabel}
      onConfirm={onConfirm}
      onCancel={onCancel}
    >
      {subjectRow}
      {nextStopLoss !== undefined && (
        <Row
          label="Stop loss"
          value={`${formatPriceOrDash(currentStopLoss)} → ${formatPriceOrDash(nextStopLoss)}`}
          fg={theme.red}
        />
      )}
      {nextTakeProfit !== undefined && (
        <Row
          label="Take profit"
          value={`${formatPriceOrDash(currentTakeProfit)} → ${formatPriceOrDash(nextTakeProfit)}`}
          fg={theme.green}
        />
      )}
    </ConfirmModal>
  );
}
