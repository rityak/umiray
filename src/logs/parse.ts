/// The levels mihomo writes, plus `raw` for everything without structure. A core panic
/// and Go runtime output arrive as plain text and must not be lost: that is exactly where
/// you see why the core did not come up.
export type Level = "debug" | "info" | "warning" | "error" | "raw";

export type Line = {
  level: Level;
  /// Time only, no date: the date is the same across the log window and takes half the line.
  time: string | null;
  text: string;
};

const LEVELS: Record<string, Level> = {
  debug: "debug",
  info: "info",
  warn: "warning",
  warning: "warning",
  error: "error",
  // The core writes fatal right before dying — coloured as an error, it needs no colour of its own.
  fatal: "error",
};

/// `msg=` comes with and without quotes: without them when the message has no spaces.
const MESSAGE = /msg=(?:"((?:[^"\\]|\\.)*)"|(\S+))/;
const LEVEL = /\blevel=(\w+)/;
const TIME = /\btime="?(\d{4}-\d{2}-\d{2})[T ](\d{2}:\d{2}:\d{2})/;

/**
 * A core log line into a shape fit for display.
 *
 * An unparsed line comes back whole with level `raw` instead of being dropped: the line we
 * did not understand is usually the most interesting one.
 */
export function parseLine(raw: string): Line {
  const level = LEVEL.exec(raw);
  const message = MESSAGE.exec(raw);
  const time = TIME.exec(raw);

  if (!level && !message) return { level: "raw", time: null, text: raw };

  return {
    level: (level && LEVELS[level[1].toLowerCase()]) || "raw",
    time: time ? time[2] : null,
    // An escaped quote inside the message comes back as is: we show the text, not the
    // source literal.
    text: (message ? (message[1] ?? message[2]) : raw).replace(/\\"/g, '"'),
  };
}

/// Level filter: "warnings" means "and worse", otherwise one would have to click twice
/// to see an error.
export const FILTERS = ["all", "warning", "error"] as const;
export type Filter = (typeof FILTERS)[number];

const RANK: Record<Level, number> = { debug: 0, info: 1, raw: 1, warning: 2, error: 3 };

export function matches(line: Line, filter: Filter, search: string): boolean {
  if (filter !== "all" && RANK[line.level] < RANK[filter]) return false;
  if (search === "") return true;
  return line.text.toLowerCase().includes(search.toLowerCase());
}
