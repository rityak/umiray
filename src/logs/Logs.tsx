import { ScrollText } from "lucide-react";
import { useMemo } from "react";
import {
  Button,
  Card,
  CopyButton,
  EmptyState,
  type LogLevel,
  type LogLine,
  LogView,
  Menu,
  MenuCheckboxItem,
} from "rootik";
import * as api from "../api";
import { useCached } from "../hooks/useCached";
import { unchanged, usePoll } from "../hooks/usePoll";
import { t } from "../i18n";
import { type Level, logSource, parseLine } from "./parse";

/// Core level as the kit's log level. Unparsed text (usually a core panic) gets no level:
/// it does not hide under a filter.
const LEVEL: Record<Level, LogLevel | undefined> = {
  trace: "trace",
  error: "error",
  warning: "warn",
  info: "info",
  debug: "debug",
  raw: undefined,
};

/// Output of the engine the sections show (D-154). Polled only while the section is open:
/// the reason for a failed start arrives in the error's `details` anyway (D-028).
export default function Logs({ engine, hidden }: { engine: api.Engine; hidden: boolean }) {
  const [raw, setRaw] = useCached<string[]>(`logs.${engine}`, []);

  const [sources, setSources] = useCached<string[]>(`logs.${engine}.sources`, [
    "umiray",
    engine,
    "volt",
  ]);
  const [levels, setLevels] = useCached<LogLevel[]>(`logs.${engine}.levels`, [
    "trace",
    "debug",
    "info",
    "warn",
    "error",
  ]);
  const labels: Record<string, string> = {
    umiray: "Umiray",
    [engine]: engine,
    volt: "VOLT",
  };

  usePoll(() => {
    api.coreLogs(engine).then(unchanged(setRaw), () => {});
  }, !hidden);

  const lines = useMemo<LogLine[]>(
    () =>
      raw.map((text) => {
        const line = parseLine(text);
        const { source, message } = logSource(line.text, engine);
        return { message, level: LEVEL[line.level], time: line.time ?? undefined, source };
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
      lines={lines
        .filter((line) => sources.includes(line.source as string))
        .map((line) => ({ ...line, source: labels[line.source as string] }))}
      levels={levels}
      onLevelsChange={setLevels}
      actions={
        <>
          <Menu
            placement="bottom-end"
            trigger={
              <Button size="sm" variant="ghost">
                {t("Log sources")} ({sources.length}/3)
              </Button>
            }
          >
            {["umiray", engine, "volt"].map((source) => (
              <MenuCheckboxItem
                key={source}
                checked={sources.includes(source)}
                onCheckedChange={(checked) =>
                  setSources((current) =>
                    checked ? [...current, source] : current.filter((item) => item !== source),
                  )
                }
              >
                {labels[source]}
              </MenuCheckboxItem>
            ))}
          </Menu>
          <CopyButton
            size="sm"
            value={() =>
              raw
                .filter((text) => sources.includes(logSource(parseLine(text).text, engine).source))
                .join("\n")
            }
            label={t("Copy log")}
          />
        </>
      }
    />
  );
}
