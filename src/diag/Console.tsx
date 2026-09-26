import { Eraser } from "lucide-react";
import { Button, type LogLevel, LogView, Spinner } from "rootik";
import type * as api from "../api";
import { t } from "../i18n";

/// One line of the feed: whose it is and what it says.
export type Entry = { tool: string; line: api.DiagLine };

type Props = {
  entries: Entry[];
  /// What is running now.
  running: string | null;
  onClear: () => void;
};

/// The tone the utility gave a line, as the kit's log level.
const LEVEL: Record<api.LineTone, LogLevel> = {
  info: "info",
  ok: "info",
  warn: "warn",
  bad: "error",
  dim: "debug",
};

/// A console at the bottom, like an IDE: an output feed with level filter, search and source.
export default function Console({ entries, running, onClear }: Props) {
  return (
    <LogView
      className="min-h-[260px] flex-1"
      label={t("Utility output")}
      showSource
      lines={entries.map((entry) => ({
        message: entry.line.text,
        level: LEVEL[entry.line.tone],
        source: entry.tool,
      }))}
      actions={
        <>
          {running !== null && <Spinner size={14} label={t("Running {tool}", { tool: running })} />}
          <Button
            size="sm"
            variant="ghost"
            icon={<Eraser />}
            disabled={entries.length === 0}
            onClick={onClear}
          >
            {t("Clear")}
          </Button>
        </>
      }
    />
  );
}
