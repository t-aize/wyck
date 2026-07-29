import type { CotSnapshot, FredSnapshot, MacroSnapshot } from "../../domain/macro.ts";
import { DOWN, FLAT, UP } from "../glyphs.ts";
import { theme } from "../theme.ts";

interface MacroPanelProps {
  macro: MacroSnapshot | undefined;
  errorMessage: string | undefined;
  fredConfigured: boolean;
}

function trend(change: number | undefined): { glyph: string; color: string } {
  if (change === undefined || change === 0) return { glyph: FLAT, color: theme.textMuted };
  return change > 0 ? { glyph: UP, color: theme.green } : { glyph: DOWN, color: theme.red };
}

/** Contrats en milliers, signé — les volumes COT se comptent par dizaines/centaines de milliers. */
function formatContracts(value: number): string {
  const sign = value >= 0 ? "+" : "";
  return `${sign}${(value / 1000).toFixed(1)}K`;
}

function CotSegment({ cot }: { cot: CotSnapshot | undefined }) {
  if (!cot) return <span fg={theme.textDim}>COT —</span>;
  const t = trend(cot.change);
  return (
    <>
      <span fg={theme.textDim}>COT (spéc.) </span>
      <span fg={theme.text}>{formatContracts(cot.net)}</span>
      <span fg={t.color}>{` ${t.glyph}`}</span>
    </>
  );
}

function FredSegment({
  label,
  snapshot,
  suffix = "",
}: {
  label: string;
  snapshot: FredSnapshot | undefined;
  suffix?: string;
}) {
  if (!snapshot) return <span fg={theme.textDim}>{`${label} —`}</span>;
  const t = trend(snapshot.change);
  return (
    <>
      <span fg={theme.textDim}>{`${label} `}</span>
      <span fg={theme.text}>{`${snapshot.value.toFixed(2)}${suffix}`}</span>
      <span fg={t.color}>{` ${t.glyph}`}</span>
    </>
  );
}

export function MacroPanel({ macro, errorMessage, fredConfigured }: MacroPanelProps) {
  return (
    <box
      title=" MACRO "
      titleColor={theme.gold}
      style={{
        flexDirection: "row",
        flexShrink: 0,
        border: true,
        borderColor: theme.border,
        backgroundColor: theme.bg,
        paddingLeft: 1,
        paddingRight: 1,
      }}
    >
      {errorMessage ? (
        <text fg={theme.red}>{errorMessage}</text>
      ) : (
        <text>
          <CotSegment cot={macro?.cot} />
          <span fg={theme.textMuted}>{"   ·   "}</span>
          {fredConfigured ? (
            <>
              <FredSegment label="DXY" snapshot={macro?.dxy} />
              <span fg={theme.textMuted}>{"   ·   "}</span>
              <FredSegment label="10Y réel" snapshot={macro?.realYield} suffix="%" />
            </>
          ) : (
            <span fg={theme.textMuted}>DXY / 10Y réel — clé FRED manquante, tape "fred"</span>
          )}
        </text>
      )}
    </box>
  );
}
