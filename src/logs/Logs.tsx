import { ScrollText } from "lucide-react";
import { useMemo } from "react";
import { Card, CopyButton, EmptyState, type LogLevel, type LogLine, LogView } from "rootik";
import * as api from "../api";
import { useCached } from "../hooks/useCached";
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

/// Output of the engine the sections show (D-154). Polled only while the section is open:
/// the reason for a failed start arrives in the error's `details` anyway (D-028).
export default function Logs({ engine, hidden }: { engine: api.Engine; hidden: boolean }) {
  const [raw, setRaw] = useCached<string[]>(`logs.${engine}`, []);

  usePoll(() => {
    api.coreLogs(engine).then(unchanged(setRaw), () => {});
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
          source: own ? "umiray" : engine,
        };
      }),
    [raw, engine],
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
        <EmptyState icon={<ScrollText />} title={t("Empty")} hint={t("The core hasn't run yet.")} />
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
