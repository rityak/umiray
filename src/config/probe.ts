import type { Rule } from "../api";

/// Rule kinds the check understands. The rest are honestly counted as skipped: a domain
/// alone says nothing about IP, country or process, and passing off "no match" as an
/// answer would be a lie.
const DOMAIN_KINDS = ["DOMAIN-SUFFIX", "DOMAIN", "DOMAIN-KEYWORD"];

export type Answer = {
  /// Index of the matching rule in the whole list; −1 — none matched.
  index: number;
  /// Which value exactly matched.
  value: string | null;
  /// How many rules the check skipped because it cannot evaluate them.
  skipped: number;
};

/**
 * Where a domain goes: the **first matching** rule from the top wins — same as in the
 * core. All values of a rule are checked, not only the first: a window rule holds
 * several, and any of them can match.
 */
export function hits(rules: Rule[], probe: string): Answer {
  const needle = probe.trim().toLowerCase();
  if (needle === "") return { index: -1, value: null, skipped: 0 };
  let skipped = 0;
  for (const [index, rule] of rules.entries()) {
    if (!DOMAIN_KINDS.includes(rule.kind)) {
      skipped += 1;
      continue;
    }
    for (const raw of rule.values) {
      const value = raw.trim().toLowerCase();
      if (value === "") continue;
      // DOMAIN-SUFFIX — the boundary is a dot: `mygithub.com` does not match `github.com`.
      const hit =
        rule.kind === "DOMAIN-SUFFIX"
          ? needle === value || needle.endsWith(`.${value}`)
          : rule.kind === "DOMAIN"
            ? needle === value
            : needle.includes(value);
      if (hit) return { index, value, skipped };
    }
  }
  return { index: -1, value: null, skipped };
}
