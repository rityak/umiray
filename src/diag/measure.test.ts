import { describe, expect, it } from "vitest";
import { measure } from "./measure";

describe("measure", () => {
  it("seconds are more than milliseconds, even when the string sorts lower", () => {
    expect(measure("1.1 s")).toBe(1100);
    expect(measure("235 ms")).toBe(235);
    expect(measure("1 с")).toBeGreaterThan(measure("235 мс") ?? 0);
  });

  it("volume and rate are in bytes", () => {
    expect(measure("12.5 KB/s")).toBe(12800);
    expect(measure("3 MB")).toBe(3 * 1024 ** 2);
    expect(measure("12,5 КБ/с")).toBe(12800);
  });

  it("a bare number is a number; text and addresses are not quantities", () => {
    expect(measure("1500")).toBe(1500);
    expect(measure("tls://dns.adguard-dns.com")).toBeNull();
    expect(measure("142.251.154.4")).toBeNull();
    expect(measure("—")).toBeNull();
  });
});
