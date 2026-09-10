/** Params de `close_position`. */
export interface ClosePositionParams {
  positionId: number;
  /**
   * Volume à clôturer en 1/100 d'unité d'actif de base
   * (`volume = lots × lotSize × 100`).
   */
  volume: number;
}
