import { useEffect, useState } from "react";

/**
 * The current time, refreshed every `ms`. For text that depends on the clock rather than
 * on data: "running 3 min", "next refresh in 20 min".
 *
 * Such text used to refresh along with a re-render of the whole window on every poll tick.
 * The window no longer re-renders on identical poll answers — and the time froze. Now only
 * whoever shows the time ticks; in a hidden window the timer stops with the page.
 */
export function useNow(ms: number, active = true): number {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const id = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(id);
  }, [ms, active]);
  return now;
}
