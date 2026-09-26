import { afterEach, expect, test, vi } from "vitest";
import { getLang, languagePreference, saveLanguagePreference, setLang } from "./i18n";

afterEach(() => vi.unstubAllGlobals());

test("a saved language overrides detection; automatic restores it", () => {
  const values = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
    removeItem: (key: string) => values.delete(key),
  });
  vi.stubGlobal("document", { documentElement: { lang: "" } });
  expect(languagePreference()).toBe("auto");
  saveLanguagePreference("en");
  setLang("ru");
  expect(getLang()).toBe("en");
  expect(languagePreference()).toBe("en");
  saveLanguagePreference("ru");
  setLang("en");
  expect(getLang()).toBe("ru");
  saveLanguagePreference("auto");
  setLang("en");
  expect(getLang()).toBe("en");
});
