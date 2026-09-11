import { TextAttributes } from "@opentui/core";
import { useMemo } from "react";
import { type InstrumentBias, instrumentBias } from "../../news/analysis/bias.ts";
import { isDefaultVisible } from "../../news/analysis/relevance.ts";
import type { CalendarEvent } from "../../news/calendar/schemas.ts";
import { PARIS_TZ, parisDayKeyFormat } from "../../news/calendar/time.ts";
import type { NewsProfile } from "../../news/profile/types.ts";
import { alignLeft, formatRelative } from "../format.ts";
import { DOWN, FLAT, UP } from "../glyphs.ts";
import { theme } from "../theme.ts";

interface NewsPanelProps {
  events: CalendarEvent[];
  errorMessage: string | undefined;
  now: Date;
  profile: NewsProfile | undefined;
}

const timeFormat = new Intl.DateTimeFormat("fr-FR", {
  timeZone: PARIS_TZ,
  hour: "2-digit",
  minute: "2-digit",
});
const dayLabelFormat = new Intl.DateTimeFormat("fr-FR", {
  timeZone: PARIS_TZ,
  weekday: "long",
  day: "numeric",
  month: "long",
});

type CalendarRow =
  | { kind: "day"; key: string; label: string }
  | { kind: "event"; key: string; event: CalendarEvent; isNext: boolean };

function buildRows(events: CalendarEvent[], now: Date, profile: NewsProfile): CalendarRow[] {
  const todayKey = parisDayKeyFormat.format(now);
  const visible = events.filter((event) => isDefaultVisible(event, profile));

  const rows: CalendarRow[] = [];
  let currentDayKey: string | undefined;
  let nextMarked = false;

  for (const event of visible) {
    const eventDate = new Date(event.timestamp);
    const dayKey = parisDayKeyFormat.format(eventDate);

    if (dayKey !== currentDayKey) {
      currentDayKey = dayKey;
      const label =
        dayKey === todayKey ? "AUJOURD'HUI" : dayLabelFormat.format(eventDate).toUpperCase();
      rows.push({ kind: "day", key: `day-${dayKey}`, label });
    }

    const isNext = !nextMarked && event.timestamp >= now.getTime();
    if (isNext) nextMarked = true;

    rows.push({
      kind: "event",
      key: `${event.date}-${event.country}-${event.title}`,
      event,
      isNext,
    });
  }

  return rows;
}

function DayHeader({ label }: { label: string }) {
  return (
    <text fg={theme.textMuted} attributes={TextAttributes.BOLD}>
      {`── ${label} `.padEnd(40, "─")}
    </text>
  );
}

function formatFigures(event: CalendarEvent): string {
  const parts: string[] = [];
  if (event.forecast?.trim()) parts.push(`F ${event.forecast}`);
  if (event.previous?.trim()) parts.push(`P ${event.previous}`);
  return parts.join("  ");
}

const BIAS_GLYPH: Record<InstrumentBias, string> = { bullish: UP, bearish: DOWN, neutral: FLAT };

function BiasBadge({ bias }: { bias: InstrumentBias | undefined }) {
  if (!bias) return <span>{"  "}</span>;
  const color = bias === "bullish" ? theme.green : bias === "bearish" ? theme.red : theme.textMuted;
  return <span fg={color}>{`${BIAS_GLYPH[bias]} `}</span>;
}

function NewsRow({
  event,
  isNext,
  now,
  profile,
}: {
  event: CalendarEvent;
  isNext: boolean;
  now: Date;
  profile: NewsProfile;
}) {
  const isPast = event.timestamp < now.getTime();
  const time = timeFormat.format(new Date(event.timestamp));
  const relative = formatRelative(event.timestamp - now.getTime());
  const figures = formatFigures(event);
  const titleColor = isNext ? theme.accent : isPast ? theme.textMuted : theme.text;

  return (
    <text truncate attributes={isNext ? TextAttributes.BOLD : TextAttributes.NONE}>
      <span fg={isNext ? theme.accent : theme.textMuted}>{isNext ? "▸ " : "  "}</span>
      <span fg={theme.textMuted}>{alignLeft(time, 6)}</span>
      <span fg={theme.textDim}>{alignLeft(event.country, 5)}</span>
      <BiasBadge bias={instrumentBias(event, profile)} />
      <span fg={titleColor}>{event.title}</span>
      {figures && <span fg={theme.textMuted}>{`  ${figures}`}</span>}
      <span fg={theme.textMuted}>{`  (${relative})`}</span>
    </text>
  );
}

export function NewsPanel({ events, errorMessage, now, profile }: NewsPanelProps) {
  const rows = useMemo(
    () => (profile ? buildRows(events, now, profile) : []),
    [events, now, profile],
  );
  const symbol = profile?.symbolName ?? "—";

  return (
    <box
      title={` CALENDAR — ${symbol} · fort impact (Paris) `}
      titleColor={theme.accent}
      bottomTitle=" ▸prochain "
      bottomTitleAlignment="right"
      flexDirection="column"
      flexGrow={2}
      flexBasis={0}
      border
      borderColor={theme.border}
      backgroundColor={theme.bg}
      paddingLeft={1}
      paddingRight={1}
    >
      {errorMessage ? (
        <text fg={theme.red}>{errorMessage}</text>
      ) : !profile ? (
        <text fg={theme.textDim}>Connexion requise pour filtrer le calendrier.</text>
      ) : rows.length === 0 ? (
        <text fg={theme.textDim}>{`Aucun événement ${symbol} / fort impact cette semaine.`}</text>
      ) : (
        <scrollbox style={{ flexGrow: 1 }}>
          {rows.map((row) =>
            row.kind === "day" ? (
              <DayHeader key={row.key} label={row.label} />
            ) : (
              <NewsRow
                key={row.key}
                event={row.event}
                isNext={row.isNext}
                now={now}
                profile={profile}
              />
            ),
          )}
        </scrollbox>
      )}
    </box>
  );
}
