import type {
  AmendablePosition,
  AmendOrderParams,
  AmendPositionParams,
  ClosablePosition,
  ClosePositionParams,
  CtraderOrder,
} from "../ctrader/schemas.ts";
import { toVolume } from "../utils/priceMath.ts";

/**
 * cTrader n'a pas d'amend partiel : tout champ non renvoyé sur `amend_order` est effacé côté
 * serveur (constaté sur limitPrice/stopPrice/SL/TP — cf. useModifyConfirm.ts). Seul point de
 * construction d'un payload amend dans l'app : reprend tout l'état resendable de l'ordre existant,
 * `changes` écrase juste ce qui doit réellement changer — impossible d'oublier un champ à un
 * nouveau point d'appel.
 *
 * Limite confirmée (vérifiée en live, compte démo, ordre GOOD_TILL_DATE réel) : `get_positions`
 * ne renvoie jamais `expirationTimestamp` pour un ordre en attente — ce n'est pas un problème de
 * nom de champ côté `CtraderOrderSchema`, la donnée est absente du payload serveur lui-même. Donc
 * `order.expirationTimestamp` ci-dessous vaut toujours `undefined` en pratique : un ordre GTD qui
 * se fait amender (même seulement SL/TP) perd silencieusement son expiration, sans qu'aucun code
 * côté client puisse la préserver faute de pouvoir la lire quelque part. À rouvrir seulement si un
 * autre endpoint (get_order_history, get_position_details) s'avère l'exposer.
 */
export function toAmendOrderParams(
  order: CtraderOrder,
  changes: Partial<Omit<AmendOrderParams, "orderId">> = {},
): AmendOrderParams {
  // Un override explicitement `undefined` (ex. proposeModify appelé sans nouveau SL) doit garder
  // la valeur existante, pas l'effacer — on ne spread que les clés réellement fournies.
  const definedChanges = Object.fromEntries(
    Object.entries(changes).filter(([, value]) => value !== undefined),
  );
  return {
    orderId: order.orderId,
    volume: order.volume,
    limitPrice: order.limitPrice,
    stopPrice: order.stopPrice,
    stopLoss: order.stopLoss,
    takeProfit: order.takeProfit,
    expirationTimestamp: order.expirationTimestamp,
    ...definedChanges,
  };
}

/** Même logique que `toAmendOrderParams`, pour une position ouverte plutôt qu'un ordre en attente.
 * `position.id` garanti défini par le type (`& { id: number }`) — c'est à l'appelant (commands/
 * amend.ts) de garder cette garantie via un type guard sur `find`, pas à cette fonction de la
 * revalider : même partage de responsabilité qu'ailleurs dans le domaine trading (validation
 * métier dans `prepare.ts`, pas ici — un id manquant est un problème de plomberie de données, pas
 * une règle de trading). */
export function toAmendPositionParams(
  position: AmendablePosition,
  changes: Partial<Omit<AmendPositionParams, "positionId">> = {},
): AmendPositionParams {
  const definedChanges = Object.fromEntries(
    Object.entries(changes).filter(([, value]) => value !== undefined),
  );
  return {
    positionId: position.id,
    stopLoss: position.stopLoss,
    takeProfit: position.takeProfit,
    ...definedChanges,
  };
}

/** Clôture totale : `volume` repris intégralement depuis `position.volumeLots` (converti via
 * `toVolume`, l'inverse de `toLots`) — pas de clôture partielle pour l'instant, cf. commands/
 * close.ts. */
export function toClosePositionParams(position: ClosablePosition): ClosePositionParams {
  return { positionId: position.id, volume: toVolume(position.volumeLots) };
}
