import { describe, expect, it } from "vitest";
import {
  asAppError,
  delayHint,
  delayLabel,
  formatBytes,
  formatUptime,
  proxyAddress,
  refreshPreset,
  runningMode,
  type Status,
  statusView,
  updatedLabel,
} from "./api";

const status = (over: Partial<Status> = {}): Status => ({
  active: null,
  running: false,
  mode: null,
  desiredMode: "local",
  restartReason: null,
  trouble: null,
  port: null,
  corePresent: true,
  qdPresent: false,
  elevated: false,
  alwaysAdmin: false,
  systemProxy: false,
  foreignProxy: null,
  autostart: false,
  killSwitch: false,
  started: null,
  ...over,
});

describe("statusView", () => {
  it("busy state takes priority", () => {
    expect(statusView(status({ running: true, mode: "tun" }), true).label).toBe("Starting…");
  });

  it("missing core takes priority over disconnected", () => {
    expect(statusView(status({ corePresent: false }), false).tone).toBe("error");
  });

  it("disconnected", () => {
    expect(statusView(status(), false)).toEqual({ label: "Disconnected", tone: "off" });
  });

  it("connected state names the mode", () => {
    expect(statusView(status({ running: true, mode: "tun" }), false).label).toContain("TUN");
    expect(statusView(status({ running: true, mode: "local" }), false).label).toContain("Proxy");
  });

  it("starting and missing core have distinct tones", () => {
    expect(statusView(status(), true).tone).toBe("connecting");
    expect(statusView(status({ corePresent: false }), false).tone).toBe("error");
  });
});

describe("proxyAddress", () => {
  it("returns an address only while running in local mode", () => {
    expect(proxyAddress(status({ running: true, mode: "local" }))).toBe("127.0.0.1:3090");
    expect(proxyAddress(status({ running: true, mode: "local", port: 7890 }))).toBe(
      "127.0.0.1:7890",
    );
  });

  // TUN captures traffic through an adapter.
  it("has no address in TUN or while stopped", () => {
    expect(proxyAddress(status({ running: true, mode: "tun" }))).toBeNull();
    expect(proxyAddress(status())).toBeNull();
  });
});

describe("formatUptime", () => {
  const at = (secondsAgo: number) => Math.floor(Date.now() / 1000) - secondsAgo;

  it("has no uptime while stopped", () => {
    expect(formatUptime(null)).toBeNull();
  });

  it("uses larger units for longer durations", () => {
    expect(formatUptime(at(12))).toBe("12 s");
    expect(formatUptime(at(90))).toBe("1 min");
    expect(formatUptime(at(4320))).toBe("1 h 12 min");
  });

  it("clock changes never produce negative uptime", () => {
    expect(formatUptime(at(-500))).toBe("0 s");
  });
});

describe("asAppError", () => {
  it("parses backend errors", () => {
    const parsed = asAppError({
      kind: "coreFailed",
      message: "Core exited (1)",
      details: ["log line"],
    });
    expect(parsed).toEqual({
      kind: "coreFailed",
      message: "Core exited (1)",
      details: ["log line"],
    });
  });

  it("handles values that are not structured errors", () => {
    expect(asAppError("plain string").kind).toBe("unknown");
    // Keep raw failures in details rather than replacing the user-facing message.
    expect(asAppError("plain string").details).toEqual(["plain string"]);
    expect(asAppError(null).details).toEqual(["null"]);
    // Missing details become an empty list.
    expect(asAppError({ kind: "io", message: "file missing" }).details).toEqual([]);
  });
});

describe("updatedLabel", () => {
  it("labels sources that have never refreshed", () => {
    const base = { id: "a1", name: "sub", url: null, updated: null, nodes: 3, records: false };
    expect(updatedLabel(base)).toBe("never refreshed");
    expect(updatedLabel({ ...base, updated: 0 })).not.toBe("never refreshed");
  });
});

describe("formatBytes", () => {
  it("uses larger units without unnecessary precision", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(999)).toBe("999 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1024 * 1024 * 3.5)).toBe("3.5 MB");
    // Three-digit values need no decimal places.
    expect(formatBytes(1024 * 1024 * 150)).toBe("150 MB");
  });
});

describe("delayLabel", () => {
  const node = (over: Partial<import("./api").Node> = {}): import("./api").Node => ({
    name: "n",
    kind: "Vless",
    source: "s",
    supported: true,
    country: null,
    delay: 120,
    method: "icmp",
    fallback: false,
    address: "a:443",
    edited: false,
    ...over,
  });

  it("shows measured delay or a dash", () => {
    expect(delayLabel(node())).toBe("120 ms");
    expect(delayLabel(node({ delay: null, method: null }))).toBe("—");
  });

  // Fallbacks must name the actual method, rather than the requested one (D-069).
  it("names the measurement method", () => {
    expect(delayHint(node({ method: "icmp" }))).toContain("ICMP");
    expect(delayHint(node({ method: "tcp" }))).toContain("TCP");
    expect(delayHint(node({ method: "proxy" }))).toContain("through the node");
    expect(delayHint(node({ delay: null, method: null }))).toContain("not measured");
  });

  it("explains when the selected check gave no result", () => {
    const hint = delayHint(node({ method: "icmp", fallback: true }));
    expect(hint).toContain("gave no result");
    expect(hint).toContain("ICMP");
  });
});

describe("runningMode", () => {
  // Desired mode belongs to config; running mode belongs to the process (D-060).
  it("reports the running mode rather than the desired mode", () => {
    expect(runningMode(status({ desiredMode: "tun" }))).toBe(null);
    expect(runningMode(status({ running: true, mode: "local", desiredMode: "tun" }))).toBe("local");
    expect(runningMode(status({ running: true, mode: "local", systemProxy: true }))).toBe("system");
    expect(runningMode(status({ running: true, mode: "tun" }))).toBe("tun");
  });
});

describe("refreshPreset", () => {
  it("recognizes presets and keeps custom schedules distinct", () => {
    expect(refreshPreset({ onStart: true, everyMinutes: 1440 })).toBe(4);
    expect(refreshPreset({ onStart: false, everyMinutes: 0 })).toBe(0);
    // An arbitrary interval must not match a preset.
    expect(refreshPreset({ onStart: true, everyMinutes: 37 })).toBe(-1);
    // The same interval without startup refresh is also custom.
    expect(refreshPreset({ onStart: false, everyMinutes: 1440 })).toBe(-1);
  });
});
