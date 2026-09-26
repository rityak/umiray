/**
 * Interface language.
 *
 * The English text is the key: code reads as English, and a missing translation falls
 * back to it instead of showing a key. Russian lives in `locales/ru.ts`; a new language
 * is one more dictionary of the same shape.
 *
 * The language is fixed for the session — picked once before the first render from the
 * keyboard layouts (`system_language`), so `t` is a plain function, not a hook.
 */

import ru from "./locales/ru";

export type Lang = "en" | "ru";

/// A translation: plain text, or three plural forms (one, few, many) for `tn`.
export type Entry = string | readonly [string, string, string];

type Vars = Record<string, string | number>;

/// Window preference (D-147); absence means automatic keyboard-layout detection.
const OVERRIDE = "umiray:lang";
export type LanguagePreference = Lang | "auto";

export function languagePreference(): LanguagePreference {
  try {
    const value = localStorage.getItem(OVERRIDE);
    return value === "en" || value === "ru" ? value : "auto";
  } catch {
    return "auto";
  }
}

export function saveLanguagePreference(value: LanguagePreference): void {
  if (value === "auto") localStorage.removeItem(OVERRIDE);
  else localStorage.setItem(OVERRIDE, value);
}

let lang: Lang = "en";

export function setLang(next: Lang): void {
  let forced: string | null = null;
  try {
    forced = localStorage.getItem(OVERRIDE);
  } catch {
    // Storage can be unavailable; the detected language stands.
  }
  lang = forced === "en" || forced === "ru" ? forced : next;
  document.documentElement.lang = lang;
}

export function getLang(): Lang {
  return lang;
}

function fill(text: string, vars?: Vars): string {
  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (match, key: string) =>
    key in vars ? String(vars[key]) : match,
  );
}

/// Translate. `{name}` placeholders are filled from `vars`.
export function t(text: string, vars?: Vars): string {
  const entry = lang === "ru" ? ru[text] : undefined;
  return fill(typeof entry === "string" ? entry : text, vars);
}

const ruRules = new Intl.PluralRules("ru");

/// Translate text that depends on a count. `{n}` is the count. The Russian entry is keyed
/// by the `other` form and holds three forms.
export function tn(n: number, one: string, other: string, vars?: Vars): string {
  const all = { n, ...vars };
  const entry = lang === "ru" ? ru[other] : undefined;
  if (Array.isArray(entry)) {
    const rule = ruRules.select(n);
    return fill(entry[rule === "one" ? 0 : rule === "few" ? 1 : 2], all);
  }
  return fill(n === 1 ? one : other, all);
}

/// Locale tag for `Intl` and `toLocaleString`.
export function locale(): string {
  return lang === "ru" ? "ru-RU" : "en-US";
}

/// Mark a text for translation without translating it yet: for tables built at module
/// load, before the language is known. Translate at render time with `t(value)`.
export const tk = (text: string): string => text;
