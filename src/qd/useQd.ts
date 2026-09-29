import { useCallback, useState } from "react";
import { unchanged, usePoll } from "../hooks/usePoll";
import * as qd from "./api";

export type Qd = { status: qd.Status | null; reload: () => void };

/// What qd says about itself: the process, rights, the tunnel. Asked only while someone
/// needs it — its sections are open or it is the one carrying traffic (the header names it).
/// Asking starts the qd process, so a window that never looks at qd never starts it.
export function useQd(needed: boolean): Qd {
  const [status, setStatus] = useState<qd.Status | null>(null);
  const reload = useCallback(() => {
    qd.status().then(unchanged(setStatus), () => {});
  }, []);
  usePoll(reload, needed);
  return { status, reload };
}
