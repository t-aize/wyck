import { TextAttributes } from "@opentui/core";
import { type CalendarEvent, classifyImpact, isGoldRelevant } from "../news.ts";
import { alignLeft, formatRelative } from "./format.ts";
import { theme } from "./theme.ts";

interface NewsPanelProps {
  events: CalendarEvent[];
  errorMessage: string | undefined;
  goldOnly: boolean;
  now: Date;
}

function impactColor(impact: ReturnType<typeof classifyImpact>): string {
  if (impact === "high") return theme.red;
  if (impact === "medium") return theme.gold;
  if (impact === "low") return theme.textDim;
  return theme.textMuted;
}

function NewsRow({ event, now }: { event: CalendarEvent; now: Date }) {
  const impact = classifyImpact(event.impact);
  const gold = isGoldRelevant(event);
  const time = new Date(event.timestamp).toLocaleTimeString("fr-FR", {
    hour: "2-digit",
    minute: "2-digit",
  });
  const relative = formatRelative(event.timestamp - now.getTime());
  const bold = impact === "high" ? TextAttributes.BOLD : TextAttributes.NONE;

  return (
    <text truncate attributes={bold}>
      <span fg={theme.textMuted}>{alignLeft(time, 6)}</span>
      <span fg={gold ? theme.gold : theme.textMuted}>{gold ? "● " : "  "}</span>
      <span fg={theme.textDim}>{alignLeft(event.country, 5)}</span>
      <span fg={impactColor(impact)}>
        {alignLeft(impact === "other" ? "" : impact.toUpperCase(), 7)}
      </span>
      <span fg={impact === "high" ? theme.text : theme.textDim}>{event.title}</span>
      <span fg={theme.textMuted}> ({relative})</span>
    </text>
  );
}

export function NewsPanel({ events, errorMessage, goldOnly, now }: NewsPanelProps) {
  const visible = goldOnly ? events.filter((event) => isGoldRelevant(event)) : events;

  return (
    <box
      title=" CALENDAR — cette semaine "
      titleColor={theme.gold}
      bottomTitle={` ●high  ●or${goldOnly ? "  [filtre or actif]" : ""} `}
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
      ) : visible.length === 0 ? (
        <text fg={theme.textDim}>chargement…</text>
      ) : (
        <scrollbox style={{ flexGrow: 1 }}>
          {visible.map((event) => (
            <NewsRow
              key={`${event.date}-${event.country}-${event.title}`}
              event={event}
              now={now}
            />
          ))}
        </scrollbox>
      )}
    </box>
  );
}
