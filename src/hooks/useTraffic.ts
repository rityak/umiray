import { useSyncExternalStore } from "react";
import type { Traffic } from "../api";

/// Шестьдесят отсчётов при такте опроса 1.5 с — это полторы минуты истории. Больше
/// на графике шириной в панель уже не различить.
export const CAPACITY = 60;

export type Speed = { down: number; up: number };

export const ZERO: Speed = { down: 0, up: 0 };

/// Две ступени с весом нового замера 1/4 сглаживают и подъём, и спад (D-150).
export const SMOOTHING = 4;

/// Предыдущее среднее переносится между тактами, чтобы сдвиг истории не менял её форму.
export function smooth(values: Speed[], window = SMOOTHING, previous = ZERO): Speed[] {
  return values.map((value) => {
    previous = {
      down: previous.down + (value.down - previous.down) / window,
      up: previous.up + (value.up - previous.up) / window,
    };
    return previous;
  });
}

/**
 * Скорость из накопительных счётчиков: ядро отдаёт всего с момента запуска, интервал
 * знает окно.
 *
 * Счётчики **сбрасываются при перезапуске ядра**, и разница уходит в минус. Без зажима
 * график получал бы выброс в обратную сторону ровно в тот момент, когда пользователь
 * переключает режим, — то есть чаще всего.
 */
export function speed(previous: Traffic, current: Traffic, elapsedMs: number): Speed {
  if (elapsedMs <= 0) return ZERO;
  const seconds = elapsedMs / 1000;
  return {
    down: Math.max(0, current.down - previous.down) / seconds,
    up: Math.max(0, current.up - previous.up) / seconds,
  };
}

export type NodeSpeed = Speed & { connections: number };

/// Скорость по узлам из двух отсчётов. Отдельно от хука — по той же причине, что и `speed`:
/// правило проверяется без React.
export function nodeSpeed(
  previous: Traffic,
  current: Traffic,
  elapsedMs: number,
): Record<string, NodeSpeed> {
  if (elapsedMs <= 0) return {};
  const seconds = elapsedMs / 1000;
  const before = new Map(previous.nodes.map((node) => [node.node, node]));
  const rates: Record<string, NodeSpeed> = {};
  for (const node of current.nodes) {
    const was = before.get(node.node);
    rates[node.node] = {
      down: Math.max(0, node.down - (was?.down ?? 0)) / seconds,
      up: Math.max(0, node.up - (was?.up ?? 0)) / seconds,
      connections: node.connections,
    };
  }
  return rates;
}

/// Что показывает «Соединение»: накопительные счётчики, история для графика, последний
/// отсчёт и скорость по узлам (S-018).
export type Live = {
  totals: Traffic | null;
  history: Speed[];
  current: Speed;
  rates: Record<string, NodeSpeed>;
};

const IDLE: Live = { totals: null, history: [], current: ZERO, rates: {} };

/// Дольше этого между отсчётами — окно было спрятано и опрос стоял. Разница за такой
/// промежуток — среднее за минуты, а не скорость: отсчёт становится новой точкой отсчёта.
const GAP_MS = 5000;

let live = IDLE;
let trend = ZERO;
let last: { traffic: Traffic; at: number } | null = null;
const listeners = new Set<() => void>();

/**
 * Принять отсчёт опроса трафика. `null` — ядро остановлено: историю чистим, иначе после
 * следующего запуска график начнётся со старого хвоста, которому уже несколько минут.
 *
 * Хранилище, а не состояние `App`: опрос идёт всё время, пока ядро работает, а не только
 * в открытом разделе, — история не должна обнуляться при уходе в «Логи» (D-076). Но
 * состояние в `App` перерисовывало всё окно каждый такт, даже раздел, которому трафик
 * безразличен. Теперь перерисовывается только тот, кто подписан.
 *
 * Скорость по узлу считается по имени, и сумма узла только растёт: байты закрывшихся
 * соединений помнит бэкенд (`core::controller::Ledger`), потому что ядро отдаёт лишь
 * открытые.
 */
export function record(traffic: Traffic | null): void {
  if (traffic === null) {
    last = null;
    trend = ZERO;
    if (live === IDLE) return;
    live = IDLE;
  } else {
    const at = Date.now();
    const previous = last !== null && at - last.at <= GAP_MS ? last : null;
    last = { traffic, at };
    // Числа над графиком берут то же среднее, чтобы два показания не спорили.
    let history = live.history;
    if (previous !== null) {
      [trend] = smooth([speed(previous.traffic, traffic, at - previous.at)], SMOOTHING, trend);
      history = [...history, ...smooth([trend], SMOOTHING, live.current)].slice(-CAPACITY);
    }
    live = {
      totals: traffic,
      history,
      current: history.at(-1) ?? ZERO,
      rates: previous === null ? {} : nodeSpeed(previous.traffic, traffic, at - previous.at),
    };
  }
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/// Живой трафик для того, кто его показывает.
export function useLive(): Live {
  return useSyncExternalStore(subscribe, () => live);
}
