import { expect, test } from "vitest";
import type { Rule } from "../api";
import { hits } from "./probe";

const rule = (kind: string, values: string[], target: string): Rule => ({
  kind,
  values,
  target,
  options: [],
});

const RULES: Rule[] = [
  rule("DOMAIN-SUFFIX", ["github.com", "gitlab.com"], "umiray"),
  rule("IP-CIDR", ["192.168.0.0/16"], "DIRECT"),
  rule("DOMAIN-SUFFIX", ["mos.ru"], "DIRECT"),
  rule("DOMAIN-KEYWORD", ["google"], "Europe"),
];

test("any value of a rule matches, not only the first", () => {
  expect(hits(RULES, "gitlab.com").index).toBe(0);
  expect(hits(RULES, "www.gitlab.com").value).toBe("gitlab.com");
});

test("DOMAIN-SUFFIX cuts at a dot, not at letters", () => {
  expect(hits(RULES, "mygithub.com").index).toBe(-1);
  expect(hits(RULES, "sub.github.com").index).toBe(0);
});

test("the first from the top wins", () => {
  const both = [rule("DOMAIN-KEYWORD", ["mos"], "umiray"), ...RULES];
  expect(hits(both, "mos.ru").index).toBe(0);
});

test("rules that cannot be checked are counted, not silent", () => {
  expect(hits(RULES, "mos.ru")).toEqual({ index: 2, value: "mos.ru", skipped: 1 });
  expect(hits(RULES, "example.org").skipped).toBe(1);
});

test("an empty query is not a miss", () => {
  expect(hits(RULES, "  ")).toEqual({ index: -1, value: null, skipped: 0 });
});
