import { ArrowDown, ArrowUp } from "lucide-react";
import { Card, EmptyState, LineChart, Stat, seriesColor, useElementSize } from "rootik";
import { formatBytes } from "../api";
import type { Speed as Rate } from "../hooks/useTraffic";
import { t } from "../i18n";
import { smooth } from "./rate";

type Props = {
  running: boolean;
  history: Rate[];
  current: Rate;
  /// Cumulative traffic since the core started.
  totals: Rate | null;
  connections?: number | null;
};

const perSecond = (value: number) => t("{rate}/s", { rate: formatBytes(value) });

/// Hide charts that would collapse into a strip.
const MIN_CHART = 56;

/// Compact labels fit the kit's 32px axis: units switch at 1000, so a label is at most "999K".
const AXIS = ["", "K", "M", "G"];
function axis(peak: number) {
  const top = Math.max(peak, 1000);
  let step = 0;
  let unit = 1;
  while (top / unit >= 1000 && step < AXIS.length - 1) {
    unit *= 1024;
    step += 1;
  }
  return {
    unit,
    format: (value: number) => (value === 0 ? "0" : `${Number(value.toPrecision(3))}${AXIS[step]}`),
  };
}

/// Stats share the series colors, making a separate legend redundant.
const DOWN = seriesColor(0);
const UP = seriesColor(1);

/// Minimum heights for stats and the empty state.
const STATS = 56;
const EMPTY = 88;

/**
 * Fill the space below connection controls. When a banner reduces height, hide the
 * chart first, then move rates into the summary without introducing page scrolling.
 */
export default function Speed({ running, history, current, totals, connections }: Props) {
  const body = useElementSize<HTMLDivElement>();
  const chart = useElementSize<HTMLDivElement>();
  const down = smooth(history.map((point) => point.down));
  const up = smooth(history.map((point) => point.up));
  const scale = axis(Math.max(0, ...down, ...up));
  const stats = body.height >= STATS;
  const summary = running
    ? [
        !stats && `↓ ${perSecond(current.down)} ↑ ${perSecond(current.up)}`,
        connections !== undefined && t("connections {n}", { n: connections ?? "—" }),
        totals && t("session {size}", { size: formatBytes(totals.down + totals.up) }),
      ]
        .filter(Boolean)
        .join(" · ")
    : body.height >= EMPTY
      ? t("last 90 seconds")
      : t("VPN is off");
  return (
    <Card padding="sm" title={t("Traffic")} description={summary} className="min-h-0 flex-1">
      <div ref={body.ref} className="h-full overflow-hidden">
        {running
          ? stats && (
              <div className="flex h-full flex-col gap-3">
                <div className="grid grid-cols-2 gap-3">
                  <Stat
                    className="um-rate"
                    size="sm"
                    label={t("Download")}
                    value={perSecond(current.down)}
                    icon={<ArrowDown style={{ color: DOWN }} />}
                  />
                  <Stat
                    className="um-rate"
                    size="sm"
                    label={t("Upload")}
                    value={perSecond(current.up)}
                    icon={<ArrowUp style={{ color: UP }} />}
                  />
                </div>
                {/* Let the chart use the remaining height. */}
                <div ref={chart.ref} className="min-h-0 flex-1 overflow-hidden">
                  {history.length > 1 && chart.height >= MIN_CHART && (
                    <LineChart
                      aria-label={t("Speed over the last 90 seconds")}
                      height={Math.floor(chart.height)}
                      legend={false}
                      area
                      format={scale.format}
                      series={[
                        {
                          name: t("Download"),
                          data: down.map((value) => value / scale.unit),
                          color: DOWN,
                        },
                        {
                          name: t("Upload"),
                          data: up.map((value) => value / scale.unit),
                          color: UP,
                        },
                      ]}
                    />
                  )}
                </div>
              </div>
            )
          : body.height >= EMPTY && (
              <div className="grid h-full place-items-center">
                <EmptyState
                  size="sm"
                  title={t("VPN is off")}
                  hint={t("The chart shows up once you connect.")}
                />
              </div>
            )}
      </div>
    </Card>
  );
}
