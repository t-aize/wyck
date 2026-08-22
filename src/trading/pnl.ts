import type { TradeSide } from "../ctrader/schemas.ts";

/**
 * P&L latent d'une position ouverte, calculé plutôt que lu : l'API cTrader (Open API
 * `ProtoOAPosition`, que ce MCP reflète — cf. commentaire sur `CtraderPositionSchema` dans
 * ctrader/schemas.ts)
 * n'expose aucun champ de profit latent, seulement des données réalisées (swap,
 * commission). Mark-to-market au bid pour un long (prix de sortie si on clôturait
 * maintenant), à l'ask pour un short — convention standard, cohérente avec le reste du
 * domaine trading qui déduit toujours le prix de référence du côté de la position.
 */
export function computeUnrealizedPnl(
  side: TradeSide,
  volumeLots: number,
  entryPrice: number,
  bid: number,
  ask: number,
): number {
  const markPrice = side === "BUY" ? bid : ask;
  const priceDiff = side === "BUY" ? markPrice - entryPrice : entryPrice - markPrice;
  return priceDiff * volumeLots * 100;
}

/** `computeUnrealizedPnl`, mais ne calcule que si les cinq entrées sont disponibles — évite de
 * retaper cette garde à chaque site d'affichage (CloseConfirmModal, PositionsPanel) qui reçoit ces
 * valeurs potentiellement absentes (mapping de position incomplet, prix pas encore chargé). */
export function computeUnrealizedPnlOrUndefined(
  side: TradeSide | undefined,
  volumeLots: number | undefined,
  entryPrice: number | undefined,
  bid: number | undefined,
  ask: number | undefined,
): number | undefined {
  if (
    side === undefined ||
    volumeLots === undefined ||
    entryPrice === undefined ||
    bid === undefined ||
    ask === undefined
  ) {
    return undefined;
  }
  return computeUnrealizedPnl(side, volumeLots, entryPrice, bid, ask);
}
