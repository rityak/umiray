import { expect, test } from "vitest";
import { resolves, retyped, ruleValues, withNoResolve } from "./rule-values";

test("bulk input preserves regex commas and process paths", () => {
  expect(ruleValues(" example.org\r\n\r\n^file{1,3}$\nC:\\Program Files\\app.exe\n")).toEqual([
    "example.org",
    "^file{1,3}$",
    "C:\\Program Files\\app.exe",
  ]);
});

test("no-resolve belongs to IP rules only and keeps the rest of the tail", () => {
  expect(resolves("IP-CIDR")).toBe(true);
  expect(resolves("GEOIP")).toBe(true);
  expect(resolves("DOMAIN-SUFFIX")).toBe(false);
  expect(withNoResolve(["src"], true)).toEqual(["src", "no-resolve"]);
  expect(withNoResolve(["src", "no-resolve"], true)).toEqual(["src", "no-resolve"]);
  expect(withNoResolve(["no-resolve", "src"], false)).toEqual(["src"]);
  // A domain rule has nothing to resolve: switching to it drops the flag.
  expect(
    retyped({ kind: "IP-CIDR", values: [], target: "DIRECT", options: ["no-resolve"] }, "DOMAIN"),
  ).toEqual({ kind: "DOMAIN", values: [], target: "DIRECT", options: [] });
  expect(
    retyped({ kind: "IP-CIDR", values: [], target: "DIRECT", options: ["no-resolve"] }, "GEOIP"),
  ).toEqual({ kind: "GEOIP", values: [], target: "DIRECT", options: ["no-resolve"] });
});
