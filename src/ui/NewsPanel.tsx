import { TextAttributes } from "@opentui/core";
import { useMemo } from "react";
import { type CalendarEvent, classifyImpact, isGoldRelevant, parisDayKey } from "../news.ts";
import { alignLeft, formatRelative } from "./format.ts";
import { theme } from "./theme.ts";

interface NewsPanelProps {
  events: CalendarEvent[];
  errorMessage: string | undefined;
  now: Date;
}

const PARIS_TZ = "Europe/Paris";
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

type Row =
  | { kind: "day"; key: string; label: string }
  | { kind: "event"; key: string; event: CalendarEvent; isNext: boolean };

/**
 * Filtres par défaut : pertinent pour l'or (USD ou mot-clé or/xau) ET impact
 * high — d'après la doc ForexFactory/marché, ce qui bouge XAUUSD en pratique,
 * c'est quasi exclusivement CPI/PCE, NFP, décisions/discours Fed (FOMC), PIB —
 * tous déjà tagués "High" côté USD. Combiner or + high revient donc à isoler
 * précisément ce sous-ensemble sans liste de mots-clés fragile à maintenir.
 * Les deux étant garanties vraies pour toute ligne affichée, on ne les répète
 * plus par ligne (● / HIGH) : ce serait redondant sur 100% des lignes.
 */
function isDefaultVisible(event: CalendarEvent): boolean {
  return isGoldRelevant(event) && classifyImpact(event.impact) === "high";
}

/**
 * Toute la semaine (déjà passé compris) : ça reste une poignée d'événements vu le
 * double filtre or+high, donc ni besoin de scroll ni de clutter — et ça évite un
 * panneau vide en fin de semaine une fois les gros événements déjà publiés.
 */
function buildRows(events: CalendarEvent[], now: Date): Row[] {
  const todayKey = parisDayKey(now);
  const visible = events.filter((event) => isDefaultVisible(event));

  const rows: Row[] = [];
  let currentDayKey: string | undefined;
  let nextMarked = false;

  for (const event of visible) {
    const eventDate = new Date(event.timestamp);
    const dayKey = parisDayKey(eventDate);

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

/** "F 20K  P 15K" — n'affiche que les valeurs présentes et non vides. */
function formatFigures(event: CalendarEvent): string {
  const parts: string[] = [];
  if (event.forecast?.trim()) parts.push(`F ${event.forecast}`);
  if (event.previous?.trim()) parts.push(`P ${event.previous}`);
  return parts.join("  ");
}

function NewsRow({ event, isNext, now }: { event: CalendarEvent; isNext: boolean; now: Date }) {
  const isPast = event.timestamp < now.getTime();
  const time = timeFormat.format(new Date(event.timestamp));
  const relative = formatRelative(event.timestamp - now.getTime());
  const figures = formatFigures(event);
  const titleColor = isNext ? theme.gold : isPast ? theme.textMuted : theme.text;

  return (
    <text truncate attributes={isNext ? TextAttributes.BOLD : TextAttributes.NONE}>
      <span fg={isNext ? theme.gold : theme.textMuted}>{isNext ? "▸ " : "  "}</span>
      <span fg={theme.textMuted}>{alignLeft(time, 6)}</span>
      <span fg={theme.textDim}>{alignLeft(event.country, 5)}</span>
      <span fg={titleColor}>{event.title}</span>
      {figures && <span fg={theme.textMuted}>{`  ${figures}`}</span>}
      <span fg={theme.textMuted}>{`  (${relative})`}</span>
    </text>
  );
}

export function NewsPanel({ events, errorMessage, now }: NewsPanelProps) {
  const rows = useMemo(() => buildRows(events, now), [events, now]);

  return (
    <box
      title=" CALENDAR — or & fort impact (Paris) "
      titleColor={theme.gold}
      bottomTitle=" ▸prochain "
      bottomTitleAlignment="right"
      style={{
        flexDirection: "column",
        flexGrow: 2,
        flexBasis: 0,
        border: true,
        borderColor: theme.border,
        backgroundColor: theme.bg,
        paddingLeft: 1,
        paddingRight: 1,
      }}
    >
      {errorMessage ? (
        <text fg={theme.red}>{errorMessage}</text>
      ) : rows.length === 0 ? (
        <text fg={theme.textDim}>Aucun événement or / fort impact cette semaine.</text>
      ) : (
        <scrollbox style={{ flexGrow: 1 }}>
          {rows.map((row) =>
            row.kind === "day" ? (
              <DayHeader key={row.key} label={row.label} />
            ) : (
              <NewsRow key={row.key} event={row.event} isNext={row.isNext} now={now} />
            ),
          )}
        </scrollbox>
      )}
    </box>
  );
}
