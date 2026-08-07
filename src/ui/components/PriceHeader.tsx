import { TextAttributes } from "@opentui/core";
import { useRef } from "react";
import { formatClock, formatMoney, formatPrice, sparkline } from "../format.ts";
import { DOWN, FLAT, UP } from "../glyphs.ts";
import { theme } from "../theme.ts";

interface PriceHeaderProps {
  symbol: string;
  bid: number | undefined;
  ask: number | undefined;
  /** Prix moyen des derniers polls (cf. useMarketData) — rendu en sparkline à côté du bid/ask. */
  priceHistory: number[];
  connected: boolean;
  now: Date;
  errorMessage: string | undefined;
  balance: number | undefined;
  moneyDigits: number | undefined;
}

export function PriceHeader({
  symbol,
  bid,
  ask,
  priceHistory,
  connected,
  now,
  errorMessage,
  balance,
  moneyDigits,
}: PriceHeaderProps) {
  const statusColor = errorMessage ? theme.red : connected ? theme.green : theme.textDim;
  const statusLabel = errorMessage ? "ERREUR" : connected ? "LIVE" : "CONNEXION…";
  const hasPrice = bid !== undefined && ask !== undefined;

  // Compare au dernier prix DIFFÉRENT pour la flèche de direction (pas au dernier rendu :
  // l'horloge fait re-rendre ce composant chaque seconde même sans nouveau prix — sans la garde
  // ci-dessous, la flèche retombait à "flat" dans la seconde suivant chaque changement, vérifié
  // en pratique). Lire/écrire des refs pendant le rendu est correct ici : simple comparaison de
  // valeur, aucun appel impératif ni dépendance au layout.
  const lastBidRef = useRef<number | undefined>(undefined);
  const previousBidRef = useRef<number | undefined>(undefined);
  if (bid !== lastBidRef.current) {
    previousBidRef.current = lastBidRef.current;
    lastBidRef.current = bid;
  }
  const direction =
    bid === undefined || previousBidRef.current === undefined
      ? undefined
      : Math.sign(bid - previousBidRef.current);
  const directionIcon =
    direction === undefined || direction === 0 ? FLAT : direction > 0 ? UP : DOWN;
  const directionColor =
    direction === undefined || direction === 0
      ? theme.textMuted
      : direction > 0
        ? theme.green
        : theme.red;

  return (
    <box
      style={{
        flexDirection: "row",
        justifyContent: "space-between",
        alignItems: "center",
        paddingLeft: 2,
        paddingRight: 2,
        height: 1,
        flexShrink: 0,
        backgroundColor: theme.panelBg,
      }}
    >
      <box style={{ flexDirection: "row", alignItems: "center", columnGap: 2 }}>
        <text attributes={TextAttributes.BOLD} fg={theme.gold}>
          AURUM
        </text>
        <text fg={theme.textDim}>{symbol}</text>
      </box>

      <box style={{ flexDirection: "row", alignItems: "center", columnGap: 1 }}>
        {hasPrice ? (
          <>
            <text fg={directionColor}>{directionIcon}</text>
            <text attributes={TextAttributes.BOLD} fg={theme.gold}>
              {formatPrice(bid)}
            </text>
            <text fg={theme.textMuted}> / {formatPrice(ask)}</text>
            {priceHistory.length >= 2 && (
              <text fg={theme.textMuted}> {sparkline(priceHistory)}</text>
            )}
          </>
        ) : (
          <text fg={theme.textDim}>{errorMessage ?? "chargement…"}</text>
        )}
      </box>

      <box style={{ flexDirection: "row", alignItems: "center", columnGap: 2 }}>
        <text fg={statusColor} attributes={TextAttributes.BOLD}>
          ● {statusLabel}
        </text>
        {balance !== undefined && moneyDigits !== undefined && (
          <text fg={theme.gold}>{formatMoney(balance, moneyDigits)}</text>
        )}
        <text fg={theme.textDim}>
          {balance !== undefined && moneyDigits !== undefined ? "· " : ""}
          {formatClock(now)}
        </text>
      </box>
    </box>
  );
}
