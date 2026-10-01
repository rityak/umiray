import { formatBytes } from "../api";
import type { NodeSpeed } from "../hooks/useTraffic";
import { t } from "../i18n";

/// What goes through the node right now: a rate, "connected" or a dash (S-018).
export function nowText(speed: NodeSpeed | undefined): string {
  if (speed === undefined) return "—";
  const rate = speed.down + speed.up;
  return rate > 0 ? t("{rate}/s", { rate: formatBytes(rate) }) : t("connected");
}

/// Binomial weights over seven samples: a peak keeps its place and height order, but its
/// edges round off — the chart reads as a wave, not a saw.
const KERNEL = [1, 6, 15, 20, 15, 6, 1];

/// The traffic chart's line, smoothed for the eye (the numbers above it stay exact). At the
/// edges the kernel shrinks to the samples that exist, so the ends do not sag to zero.
export function smooth(values: readonly number[]): number[] {
  const half = (KERNEL.length - 1) / 2;
  return values.map((_, i) => {
    let sum = 0;
    let weight = 0;
    KERNEL.forEach((w, k) => {
      const value = values[i + k - half];
      if (value === undefined) return;
      sum += value * w;
      weight += w;
    });
    return sum / weight;
  });
}
