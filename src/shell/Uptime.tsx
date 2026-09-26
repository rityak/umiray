import { formatUptime } from "../api";
import { useNow } from "../hooks/useNow";

/// How long the core has been running — on its own clock (`useNow`): only this text ticks
/// once a second, not its parent.
export default function Uptime({
  started,
  fallback,
}: {
  started: number | null;
  fallback: string;
}) {
  const now = useNow(1000, started !== null);
  return formatUptime(started, now) ?? fallback;
}
