import { useCallback, useRef, useState } from "react";
import * as api from "../api";
import { has } from "../features";
import { unchanged, usePoll } from "../hooks/usePoll";
import { failure, type Message } from "../shell/Banner";

export function useVolt(report: (message: Message | null) => void) {
  const [snapshot, setSnapshot] = useState<api.VoltSnapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const reload = useCallback(async () => {
    if (pending.current) return;
    try {
      unchanged(setSnapshot)(await api.voltGet());
    } catch (error) {
      report(failure(error));
    }
  }, [report]);
  // VOLT есть только там, где ОС его умеет (D-174): спрашивать о нём незачем.
  usePoll(
    () => {
      void reload();
    },
    has("volt"),
    3000,
  );
  const save = useCallback(
    async (options: api.VoltOptions) => {
      if (pending.current) return false;
      pending.current = true;
      setBusy(true);
      report(null);
      try {
        setSnapshot(await api.voltSet(options));
        return true;
      } catch (error) {
        report(failure(error));
        try {
          setSnapshot(await api.voltGet());
        } catch {
          /* Keep the original failure visible. */
        }
        return false;
      } finally {
        pending.current = false;
        setBusy(false);
      }
    },
    [report],
  );
  const tune = useCallback(async () => {
    if (pending.current) return null;
    pending.current = true;
    setBusy(true);
    report(null);
    try {
      const result = await api.voltTune();
      setSnapshot(await api.voltGet());
      return result;
    } catch (error) {
      report(failure(error));
      return null;
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }, [report]);
  return { snapshot, busy, reload, save, tune };
}
export type VoltController = ReturnType<typeof useVolt>;
