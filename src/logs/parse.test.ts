import { describe, expect, it } from "vitest";
import { type Line, matches, parseLine } from "./parse";

describe("parseLine", () => {
  it("parses a regular core line", () => {
    const line = parseLine(
      'time="2026-08-28T20:11:03+03:00" level=info msg="Mixed(http+socks) proxy listening at: 127.0.0.1:3090"',
    );
    expect(line).toEqual({
      level: "info",
      time: "20:11:03",
      text: "Mixed(http+socks) proxy listening at: 127.0.0.1:3090",
    });
  });

  it("folds warn and warning into one level, fatal into error", () => {
    expect(parseLine('level=warn msg="x"').level).toBe("warning");
    expect(parseLine('level=warning msg="x"').level).toBe("warning");
    expect(parseLine('level=fatal msg="x"').level).toBe("error");
  });

  it("understands msg without quotes", () => {
    expect(parseLine("level=debug msg=started").text).toBe("started");
  });

  it("returns an escaped quote as text, not as a literal", () => {
    expect(parseLine('level=error msg="Get \\"url\\": EOF"').text).toBe('Get "url": EOF');
  });

  // A Go panic arrives without structure — and it is exactly the line the log is opened for.
  it("returns an unparsed line whole instead of dropping it", () => {
    const panic = "panic: runtime error: invalid memory address";
    expect(parseLine(panic)).toEqual({ level: "raw", time: null, text: panic });
  });

  it("does not crash on an empty line", () => {
    expect(parseLine("")).toEqual({ level: "raw", time: null, text: "" });
  });
});

describe("matches", () => {
  const line = (over: Partial<Line> = {}): Line => ({
    level: "info",
    time: null,
    text: "match Match using Sweden 0",
    ...over,
  });

  it("warnings shows errors too", () => {
    expect(matches(line({ level: "error" }), "warning", "")).toBe(true);
    expect(matches(line({ level: "warning" }), "warning", "")).toBe(true);
    expect(matches(line({ level: "info" }), "warning", "")).toBe(false);
  });

  it("searches case-insensitively", () => {
    expect(matches(line(), "all", "SWEDEN")).toBe(true);
    expect(matches(line(), "all", "netherlands")).toBe(false);
  });

  // An unparsed line must not hide behind the level filter the way debug does: with the
  // "all" filter it is visible, and that is what matters.
  it("an unparsed line is visible with the all filter", () => {
    expect(matches(line({ level: "raw" }), "all", "")).toBe(true);
  });
});
