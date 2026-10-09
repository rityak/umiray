import type * as api from "../api";
import { t } from "../i18n";

/// Выходы клиента — первые узлы списка (D-166). Источника у них нет, это и есть их примета:
/// их не правят, не меряют и не сортируют.
const DIRECT = "DIRECT";
const AUTO = "AUTO";

const exit = (name: string, kind: string, supported: boolean): api.Node => ({
  name,
  kind,
  source: "",
  supported,
  delay: null,
  method: null,
  fallback: false,
  address: null,
  country: null,
  edited: false,
});

/// DIRECT и AUTO. Без узлов AUTO выбирать не из чего — он виден, но не нажимается.
export function exits(nodes: number): api.Node[] {
  return [
    exit(DIRECT, t("bypass proxy"), true),
    exit(AUTO, t("spread across working nodes"), nodes > 0),
  ];
}

export function isExit(node: api.Node): boolean {
  return node.source === "";
}

/// Какая строка списка отмечена: выход по направлению, узел — в Manual.
export function chosen(direction: api.Direction, node: string | null): string | null {
  if (direction === "direct") return DIRECT;
  if (direction === "auto") return AUTO;
  return node;
}

/// Нажатие по строке списка — направление, а по узлу ещё и сам узел.
export function choice(name: string): { direction: api.Direction; node?: string } {
  if (name === DIRECT) return { direction: "direct" };
  if (name === AUTO) return { direction: "auto" };
  return { direction: "manual", node: name };
}

/// Выход DIRECT через обход называется своим выходом ядра (D-186); подпись — как он идёт.
export function bypassNote(target: string): string | null {
  if (target === "DIRECT-AUTO") return t("without proxy, bypass when needed");
  if (target === "DIRECT-VOLT") return t("without proxy, always bypassing");
  return null;
}
