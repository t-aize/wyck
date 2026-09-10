/** Params de `amend_position` (SL / TP d'une position déjà ouverte). */
export interface AmendPositionParams {
  positionId: number;
  /** Nouveau SL en prix affiché (pas en pipettes) ; omis = inchangé. */
  stopLoss?: number;
  /** Nouveau TP en prix affiché ; omis = inchangé. */
  takeProfit?: number;
  trailingStopLoss?: boolean;
}
