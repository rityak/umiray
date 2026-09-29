/// Данные — словами: состояние, объёмы, время, задержки. Без обращений к бэкенду.

import { locale, t, tk, tn } from "../i18n";
import { MODE_LABEL, runningMode, type Status } from "./core";
import { type Node, PING_HINT } from "./nodes";
import type { Import, Source } from "./sources";

/// Четыре состояния из STYLEGUIDE, и ровно четыре: «подключается» и «ошибка» — разные вещи,
/// раньше они делили один `warn`.
export type Tone = "on" | "connecting" | "off" | "error";

/** Состояние показываем текстом и цветом, а не одним цветом (STYLEGUIDE). */
export function statusView(status: Status, busy: boolean): { label: string; tone: Tone } {
  if (busy) return { label: t("Starting…"), tone: "connecting" };
  if (!status.corePresent) return { label: t("Core not found"), tone: "error" };
  const mode = runningMode(status);
  if (mode === null) return { label: t("Disconnected"), tone: "off" };
  // Адрес здесь не повторяем: он показан отдельно и с кнопкой копирования, а в двух
  // местах сразу одно и то же значение только сбивает.
  return { label: `${t("Connected")} · ${MODE_LABEL[mode]}`, tone: "on" };
}

/**
 * Сколько ядро работает, словами. Считается от отметки в статусе, а не таймером в окне:
 * окно живёт дольше ядра и переживает его перезапуск (D-057).
 *
 * Секунд не показываем дальше первой минуты: строка в шапке меняется раз в полторы
 * секунды, и бегущие цифры в ней читались бы как тревога, а не как справка.
 */
export function formatUptime(started: number | null, now = Date.now()): string | null {
  if (started === null) return null;
  const seconds = Math.max(0, Math.floor(now / 1000) - started);
  if (seconds < 60) return t("{n} s", { n: seconds });
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return t("{n} min", { n: minutes });
  const hours = Math.floor(minutes / 60);
  return t("{hours} h {minutes} min", { hours, minutes: minutes % 60 });
}

const UNITS = [tk("B"), tk("KB"), tk("MB"), tk("GB"), tk("TB")];

/** Объём человеческим языком. Ядро отдаёт байты, читать их глазами невозможно. */
export function formatBytes(bytes: number): string {
  let value = Math.max(0, bytes);
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1;
  return `${value.toFixed(digits)} ${t(UNITS[unit])}`;
}

/** Итог импорта одной строкой. */
export function importSummary(result: Import): string {
  return `«${result.source.name}»: ${tn(result.source.nodes, "{n} node", "{n} nodes")}`;
}

/** Когда источник последний раз обновляли. Никогда — значит собран из отдельных ссылок. */
export function updatedLabel(source: Source): string {
  if (source.updated === null) return t("never refreshed");
  return new Date(source.updated * 1000).toLocaleString(locale());
}

/**
 * Сколько до сервера. Ядро тут ни при чём — меряем сами, и работает это при выключенном
 * VPN тоже (D-062).
 */
export function delayLabel(node: Node): string {
  return node.delay === null ? "—" : t("{n} ms", { n: node.delay });
}

/// Чем именно померено — словом, а не только цветом (STYLEGUIDE).
///
/// Запасной замер называет себя первым: число в такой строке отвечает не на тот вопрос,
/// который задавали, и молча выдавать его за заказанный нельзя (D-069).
export function delayHint(node: Node): string {
  if (node.method === null) return t("not measured or the server did not respond");
  if (node.fallback) {
    // У `hysteria2`, `tuic` и `wireguard` TCP-порта нет вовсе, и «проверка не дала
    // результата» про них — не про сервер, а про сам способ. Такие меряет ядро.
    if (node.method === "proxy" || node.method === "proxy-keepalive") {
      return t("This protocol has no TCP response — measured through the node: {method}", {
        method: t(PING_HINT[node.method]),
      });
    }
    return t("The selected check gave no result, but the host is reachable — {method}", {
      method: t(PING_HINT[node.method]),
    });
  }
  return t(PING_HINT[node.method]);
}
