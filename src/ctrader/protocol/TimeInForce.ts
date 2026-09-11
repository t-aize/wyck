/**
 * Validité temporelle d'un ordre, envoyée à `create_order`.
 *
 * - {@link TimeInForce.GOOD_TILL_CANCEL} — reste jusqu'à annulation / fill.
 * - {@link TimeInForce.GOOD_TILL_DATE} — expire à `expirationTimestamp` (epoch ms).
 * - {@link TimeInForce.IMMEDIATE_OR_CANCEL} — fill immédiat, le reliquat est annulé.
 */
export enum TimeInForce {
  GOOD_TILL_CANCEL = "GOOD_TILL_CANCEL",
  GOOD_TILL_DATE = "GOOD_TILL_DATE",
  IMMEDIATE_OR_CANCEL = "IMMEDIATE_OR_CANCEL",
}
