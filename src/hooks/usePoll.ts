import { type Dispatch, type SetStateAction, useEffect, useRef } from "react";
import { POLL_MS } from "../api";

/**
 * One poll tick for the whole app (D-007: there is no other way to get live data from the
 * core — the backend sends no events yet). This loop used to be copied in three places verbatim.
 *
 * The handler is kept in a ref: otherwise every parent re-render would recreate the interval,
 * and the tick would stumble exactly when data changes most often.
 *
 * A hidden window polls nothing: the client lives in the tray for hours (D-046), and every
 * tick is backend calls and a config assembly for a status nobody sees. The tray and the
 * supervisor live in Rust and do not wait for the window's poll. Window shown — the tick
 * runs at once.
 */
export function usePoll(tick: () => void, active = true, ms = POLL_MS) {
  const latest = useRef(tick);
  latest.current = tick;

  useEffect(() => {
    if (!active) return;
    let id: ReturnType<typeof setInterval> | undefined;
    const sync = () => {
      clearInterval(id);
      id = undefined;
      if (document.hidden) return;
      latest.current();
      id = setInterval(() => latest.current(), ms);
    };
    sync();
    document.addEventListener("visibilitychange", sync);
    return () => {
      clearInterval(id);
      document.removeEventListener("visibilitychange", sync);
    };
  }, [active, ms]);
}

/**
 * A setter that leaves the state alone when the poll brought the same thing.
 *
 * The poll brings a **new** object every tick even when nothing changed — and React
 * dutifully re-rendered everything under it every second and a half. A JSON comparison is
 * cheaper than re-rendering the node list or a five-hundred-line log.
 */
export function unchanged<T>(set: Dispatch<SetStateAction<T>>): (next: T) => void {
  return (next) =>
    set((current) => (JSON.stringify(current) === JSON.stringify(next) ? current : next));
}
