import { toLots } from "../constants.ts";
import type { CtraderPosition, TradeSide } from "./schemas.ts";

/**
 * `CtraderPosition` reste typé `Record<string, unknown>` par choix délibéré (cf. le
 * commentaire en tête de `ctrader/schemas.ts`), pas par manque de données : sa forme
 * réelle a été vérifiée contre le compte démo, tous les champs ci-dessous confirmés
 * présents et nommés ainsi : `positionId, symbolId, tradeSide, volume, entryPrice,
 * stopLoss?, takeProfit?, commission, swap`. Pas de champ d'horodatage d'ouverture sur
 * cette forme (contrairement à ce qu'un premier recoupement avec le proto Open API
 * public de Spotware suggérait) — ce mapper ne lit donc pas d'"âge" de position, cf.
 * `PositionsPanel.tsx` qui n'a plus de colonne AGE pour cette raison.
 *
 * Le prix d'entrée est exposé sous `entryPrice`, pas `price` comme le même recoupement
 * proto le suggérait initialement — avec l'ancien nom, `readNumber(position, ["price"])`
 * ne résolvait *jamais* sur un vrai payload, donc `read.entry` restait toujours
 * `undefined` et `isUnmapped()` déclenchait l'avertissement "position non reconnue" sur
 * *chaque* position réelle.
 *
 * Toujours aucun champ de P&L latent sur cette forme — cf. `computeUnrealizedPnl` dans
 * domain/trading.ts, qui le calcule plutôt que de lire un nom de champ inexistant.
 *
 * Filet de sécurité : `App.tsx`/`PositionsPanel.tsx` vérifient que positionId/tradeSide/
 * volume/entry résolvent bien sur une liste de positions non vide, et affichent un
 * avertissement visible sinon — pour ne plus jamais échouer en silence si cette forme
 * change côté serveur.
 */
function readNumber(record: Record<string, unknown>, keys: string[]): number | undefined {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "number") return value;
  }
  return undefined;
}

function readTradeSide(record: Record<string, unknown>): TradeSide | undefined {
  const value = record.tradeSide;
  return value === "BUY" || value === "SELL" ? value : undefined;
}

export interface ReadPosition {
  id: number | undefined;
  side: TradeSide | undefined;
  volumeLots: number | undefined;
  entry: number | undefined;
  stopLoss: number | undefined;
  takeProfit: number | undefined;
  swap: number | undefined;
}

export function readPosition(position: CtraderPosition): ReadPosition {
  const volume = readNumber(position, ["volume"]);
  return {
    id: readNumber(position, ["positionId"]),
    side: readTradeSide(position),
    // lotSize métaux = 100 → volume(1/100 unités) / 10000 = lots. Approximation valable pour XAUUSD,
    // seul symbole tradé par ce panel — à revoir si d'autres classes d'actifs sont ajoutées un jour.
    volumeLots: volume === undefined ? undefined : toLots(volume),
    entry: readNumber(position, ["entryPrice"]),
    stopLoss: readNumber(position, ["stopLoss"]),
    takeProfit: readNumber(position, ["takeProfit"]),
    swap: readNumber(position, ["swap"]),
  };
}

/** Une position dont les champs à haute confiance (cf. commentaire en tête de fichier) ne
 * résolvent pas du tout signale que l'hypothèse de mapping est cassée en pratique. */
export function isUnmapped(read: ReadPosition): boolean {
  return (
    read.id === undefined ||
    read.side === undefined ||
    read.volumeLots === undefined ||
    read.entry === undefined
  );
}
