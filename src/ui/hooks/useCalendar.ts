import { type CalendarEvent, fetchCalendar } from "@aurum/news";
import { useMemo, useState } from "react";
import { APP_DATA_DIR } from "../../constants.ts";
import { fsRuntime } from "../../utils/effectRuntime.ts";
import { toMessage } from "../../utils/errors.ts";
import { useInterval } from "./useInterval.ts";

const NEWS_POLL_MS = 5 * 60_000;

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
