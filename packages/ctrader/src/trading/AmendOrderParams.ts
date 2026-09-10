import type { OrderPriceFields } from "./OrderPriceFields.ts";

/**
 * Params de `amend_order` — payload **complet**, pas un patch.
 * Tout champ omis est effacé côté serveur.
 */
export interface AmendOrderParams extends OrderPriceFields {
  orderId: number;
  volume?: number;
}
