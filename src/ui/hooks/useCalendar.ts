import { useMemo, useState } from "react";
import { type CalendarEvent, fetchCalendar } from "../../domain/news.ts";
import { fsRuntime } from "../../effectRuntime.ts";
import { toMessage } from "../../errors.ts";
import { useInterval } from "./useInterval.ts";

const NEWS_POLL_MS = 5 * 60_000;

export interface Calendar {
  calendar: CalendarEvent[];
  newsError: string | undefined;
  refreshNews: (options?: { force?: boolean }) => Promise<void>;
}

export function useCalendar(): Calendar {
  const [calendar, setCalendar] = useState<CalendarEvent[]>([]);
  const [newsError, setNewsError] = useState<string>();

  const refreshNews = useMemo(
    () =>
      async (options: { force?: boolean } = {}) => {
        try {
          setCalendar(await fsRuntime.runPromise(fetchCalendar(options)));
          setNewsError(undefined);
        } catch (error) {
          setNewsError(toMessage(error));
        }
      },
    [],
  );

  useInterval(() => void refreshNews(), NEWS_POLL_MS);

  return { calendar, newsError, refreshNews };
}
