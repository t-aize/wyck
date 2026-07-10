import { TextAttributes } from "@opentui/core";
import { formatClock, formatPrice } from "./format.ts";
import { theme } from "./theme.ts";

interface PriceHeaderProps {
  symbol: string;
  bid: number | undefined;
  ask: number | undefined;
  connected: boolean;
  now: Date;
  errorMessage: string | undefined;
}

export function PriceHeader({ symbol, bid, ask, connected, now, errorMessage }: PriceHeaderProps) {
  const statusColor = errorMessage ? theme.red : connected ? theme.green : theme.textDim;
  const statusLabel = errorMessage ? "ERREUR" : connected ? "LIVE" : "CONNEXION…";
  const hasPrice = bid !== undefined && ask !== undefined;

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
            <text attributes={TextAttributes.BOLD} fg={theme.gold}>
              {formatPrice(bid)}
            </text>
            <text fg={theme.textMuted}> / {formatPrice(ask)}</text>
          </>
        ) : (
          <text fg={theme.textDim}>{errorMessage ?? "chargement…"}</text>
        )}
      </box>

      <box style={{ flexDirection: "row", alignItems: "center", columnGap: 2 }}>
        <text fg={statusColor} attributes={TextAttributes.BOLD}>
          ● {statusLabel}
        </text>
        <text fg={theme.textDim}>{formatClock(now)}</text>
      </box>
    </box>
  );
}
