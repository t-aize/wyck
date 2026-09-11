/**
 * Forme observée des 5 outils d'écriture (compte démo, XAUUSD) :
 * `{ orderId, positionId, executionType, order, position, deal? }`.
 *
 * L'app ne lit **aucun** de ces champs — seulement succès / échec de la
 * Promise. On modèle le minimum pour documenter, sans prétendre à un contrat
 * exhaustif (`executionType` n'est pas un enum : rejets / partiels jamais vus
 * pendant les tests).
 *
 * - `order` / `position` ne décrivent pas toujours l'action « principale » :
 *   `close_position` peut renvoyer `ORDER_CANCELLED` pour le SL/TP auto, pas
 *   le MARKET qui a clôturé.
 * - `deal` (souvent absent) : `{ dealId, volume, closePrice }` si l'appel
 *   déclenche une exécution immédiate.
 */
export interface WriteResult {
  orderId?: number;
  positionId?: number;
  executionType?: string;
  /** Champs supplémentaires du serveur (order, position, deal…) — non modélisés. */
  [key: string]: unknown;
}
