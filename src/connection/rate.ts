import { formatBytes } from "../api";
import type { NodeSpeed } from "../hooks/useTraffic";
import { t } from "../i18n";

/// What goes through the node right now: a rate, "connected" or a dash (S-018).
export function nowText(speed: NodeSpeed | undefined): string {
  if (speed === undefined) return "—";
  const rate = speed.down + speed.up;
  return rate > 0 ? t("{rate}/s", { rate: formatBytes(rate) }) : t("connected");
}
