import { toLots } from "../constants.ts";
import type { CtraderPosition, TradeSide } from "./schemas.ts";

/**
 * `CtraderPosition` reste typé `Record<string, unknown>` : sa forme exacte n'a jamais pu
 * être exercée contre un payload réel (pas de compte démo, aucune position ouverte lors
 * des tests). Les noms de champs ci-dessous ne sont donc pas devinés au hasard : ils
 * croisent deux sources indépendantes — (1) le proto Open API public de Spotware
 * (`ProtoOAPosition`/`ProtoOATradeData` : positionId, tradeData.{symbolId, volume,
 * tradeSide, openTimestamp}, swap, price, stopLoss, takeProfit, commission,
 * utcLastUpdateTimestamp — https://github.com/spotware/openapi-proto-messages) et (2) le
 * pattern déjà confirmé sur `CtraderOrder`/`CtraderDeal` (vérifiés contre de vrais
 * payloads) : ce MCP aplatit `tradeData.*` directement sur l'objet JSON, en camelCase,
 * sans renommage nulle part ailleurs dans cette API. Confiance haute sur positionId /
 * tradeSide / volume / price / stopLoss / takeProfit / swap / openTimestamp. Le proto Open
 * API n'a en revanche *aucun* champ de P&L latent — ce n'est pas un oubli côté lecteur,
 * il n'y a rien à lire : cf. `computeUnrealizedPnl` dans domain/trading.ts, qui le calcule
 * plutôt que de deviner un nom de champ inexistant.
 *
 * Filet de sécurité : `App.tsx`/`PositionsPanel.tsx` vérifient que positionId/tradeSide/
 * volume/price résolvent bien sur une liste de positions non vide, et affichent un
 * avertissement visible sinon — pour ne plus jamais échouer en silence si cette hypothèse
 * s'avère fausse en pratique.
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
  openTimestamp: number | undefined;
}

export function readPosition(position: CtraderPosition): ReadPosition {
  const volume = readNumber(position, ["volume"]);
  return {
    id: readNumber(position, ["positionId"]),
    side: readTradeSide(position),
    // lotSize métaux = 100 → volume(1/100 unités) / 10000 = lots. Approximation valable pour XAUUSD,
    // seul symbole tradé par ce panel — à revoir si d'autres classes d'actifs sont ajoutées un jour.
    volumeLots: volume === undefined ? undefined : toLots(volume),
    entry: readNumber(position, ["price"]),
    stopLoss: readNumber(position, ["stopLoss"]),
    takeProfit: readNumber(position, ["takeProfit"]),
    swap: readNumber(position, ["swap"]),
    openTimestamp: readNumber(position, ["openTimestamp"]),
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
