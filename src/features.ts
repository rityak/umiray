/**
 * What this OS does well (D-174). Fixed for the session — asked once before the first render,
 * like the language — so `has` is a plain function, not a hook.
 *
 * The window hides what is missing and never branches on the OS name: a control for a feature
 * goes behind `has("…")`, `<Supported>`, or into `HIDES` when it is a settings row.
 */

import { z } from "zod";

export const Feature = z.enum([
  "qd",
  "volt",
  "alwaysAdmin",
  "systemProxy",
  "killSwitch",
  "icmpPing",
  "mtuProbe",
  "autostart",
  "selfUpdate",
]);
export type Feature = z.infer<typeof Feature>;

/// The backend's list (`system_features`); unknown names are refused at the border.
export const Features = z.array(Feature);

/// Before the answer (and if the backend never answers) everything is shown: a missing
/// feature still refuses with its reason, an absent button cannot even do that.
let supported: ReadonlySet<Feature> = new Set(Feature.options);

export function has(feature: Feature): boolean {
  return supported.has(feature);
}

/// For lists of choices: an item that names a feature stays only where the OS has it.
export function available(item: { feature?: Feature }): boolean {
  return item.feature === undefined || has(item.feature);
}

export function setFeatures(list: readonly Feature[]): void {
  supported = new Set(list);
}

/// Settings rows (`Page` ids) that only make sense with the feature.
const HIDES: Record<Feature, string[]> = {
  qd: ["service-qd"],
  volt: ["volt-form"],
  alwaysAdmin: ["always-admin"],
  systemProxy: [],
  killSwitch: ["guard-firewall"],
  icmpPing: [],
  mtuProbe: [],
  autostart: ["autostart"],
  selfUpdate: [],
};

/// Rows to hide on this OS.
export function unsupportedRows(): string[] {
  return Feature.options.filter((feature) => !has(feature)).flatMap((feature) => HIDES[feature]);
}
