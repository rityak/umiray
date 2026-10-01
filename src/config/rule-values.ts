import type * as api from "../api";

/** D-148: newlines separate entries; commas and spaces inside values stay literal. */
export function ruleValues(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

const NO_RESOLVE = "no-resolve";

/// Rule kinds that match an address. A connection that arrives with a name makes the core
/// look the name up first — unless the rule says `no-resolve`. Domain rules never look up.
const BY_ADDRESS = ["IP-CIDR", "IP-CIDR6", "IP-SUFFIX", "IP-ASN", "GEOIP"];

export function resolves(kind: string): boolean {
  return BY_ADDRESS.includes(kind);
}

/// The rule's tail with `no-resolve` on or off; the rest of the tail stays as written.
export function withNoResolve(options: string[], on: boolean): string[] {
  const rest = options.filter((option) => option !== NO_RESOLVE);
  return on ? [...rest, NO_RESOLVE] : rest;
}

/// A new kind for a rule. A kind that never looks names up drops `no-resolve`: the flag
/// would stay in the file meaning nothing.
export function retyped(rule: api.Rule, kind: string): api.Rule {
  return {
    ...rule,
    kind,
    options: resolves(kind) ? rule.options : withNoResolve(rule.options, false),
  };
}
