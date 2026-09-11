import { useMemo, useState } from "react";
import { APP_DATA_DIR } from "../../constants.ts";
import { CALENDAR_CACHE_TTL_MS, fetchCalendar } from "../../news/calendar/fetch.ts";
import type { CalendarEvent } from "../../news/calendar/schemas.ts";
import { fsRuntime } from "../../utils/effectRuntime.ts";
import { toMessage } from "../../utils/errors.ts";
import { useInterval } from "./useInterval.ts";

/** Plus court que le TTL cache : un tick sur cache frais est un read disque, pas un fetch. */
const NEWS_POLL_MS = Math.min(5 * 60_000, CALENDAR_CACHE_TTL_MS);

interface Calendar {
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
          setCalendar(
            await fsRuntime.runPromise(
              fetchCalendar({ cacheDir: APP_DATA_DIR, force: options.force }),
            ),
          );
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
