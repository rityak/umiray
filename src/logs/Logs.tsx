import { ScrollText } from "lucide-react";
import { useMemo, useState } from "react";
import { Card, CopyButton, EmptyState, type LogLevel, type LogLine, LogView } from "rootik";
import * as api from "../api";
import { unchanged, usePoll } from "../hooks/usePoll";
import { t } from "../i18n";
import { type Level, parseLine } from "./parse";

/// Core level as the kit's log level. Unparsed text (usually a core panic) gets no level:
/// it does not hide under a filter.
const LEVEL: Record<Level, LogLevel | undefined> = {
  error: "error",
  warning: "warn",
  info: "info",
  debug: "debug",
  raw: undefined,
};

/// Lines the client writes itself (`supervisor::note`): they get lost among the core's
/// output, yet they are exactly what explains what a button press did. The source names them.
const OWN = "umiray:";

/// Core output. Polled only while the section is open: the reason for a failed start
/// arrives in the error's `details` anyway (D-028).
export default function Logs({ hidden }: { hidden: boolean }) {
  const [raw, setRaw] = useState<string[]>([]);

  usePoll(() => {
    api.coreLogs().then(unchanged(setRaw), () => {});
  }, !hidden);

  const lines = useMemo<LogLine[]>(
    () =>
      raw.map((text) => {
        const line = parseLine(text);
        const own = line.text.startsWith(OWN);
        return {
          message: own ? line.text.slice(OWN.length).trim() : line.text,
          level: LEVEL[line.level],
          time: line.time ?? undefined,
          source: own ? "umiray" : "mihomo",
        };
      }),
    [raw],
  );

  if (hidden) {
    return (
      <Card>
        <EmptyState
          title={t("Log hidden")}
          hint={t("Turn off private mode: lines may contain node addresses and names.")}
        />
      </Card>
    );
  }

  if (raw.length === 0) {
    return (
      <Card>
        <EmptyState
          icon={<ScrollText />}
          title={t("Empty")}
          hint={t("The core has not run — nothing to write.")}
        />
      </Card>
    );
  }

  return (
    <LogView
      className="min-h-0 flex-1"
      label={t("Core log")}
      showTime
      showSource
      // Time arrives as an "hh:mm:ss" string — shown as is.
      timeFormat={(time) => String(time)}
      lines={lines}
      actions={<CopyButton size="sm" value={() => raw.join("\n")} label={t("Copy log")} />}
    />
  );
}
