/// Инструменты (D-097, D-115): утилита и её отчёт одним типом.

import { z } from "zod";
import { call } from "./call";

export const Verdict = z.enum(["ok", "warn", "bad", "idle"]);
export type Verdict = z.infer<typeof Verdict>;
/// Тон строки консоли. Цвет подбирает окно: модуль про темы ничего не знает.
/// Имя со словом «строка», потому что `Tone` в этом файле уже занят состоянием
/// подключения — а это разные вещи, и путать их нельзя.
export const LineTone = z.enum(["info", "ok", "warn", "bad", "dim"]);
export type LineTone = z.infer<typeof LineTone>;

export const Tool = z.object({
  id: z.string(),
  title: z.string(),
  /// Группа в списке слева.
  group: z.string(),
  hint: z.string(),
  /// Какие параметры принимает — по ним рисуется полоса над консолью.
  params: z.array(z.string()),
  /// Ходит ли в сеть.
  network: z.boolean(),
});
export type Tool = z.infer<typeof Tool>;

export const DiagLine = z.object({ tone: LineTone, text: z.string() });
export type DiagLine = z.infer<typeof DiagLine>;
/// Строка таблицы. `mark` — то, что утилита предлагает взять: на нём стоит действие.
export const DiagRow = z.object({
  cells: z.array(z.string()),
  verdict: Verdict,
  mark: z.boolean(),
});
export type DiagRow = z.infer<typeof DiagRow>;

export const Report = z.object({
  tool: z.string(),
  verdict: Verdict,
  /// Одна строка для «Проверки»: не «ок», а что именно нашлось.
  headline: z.string(),
  ms: z.number(),
  columns: z.array(z.string()),
  rows: z.array(DiagRow),
  lines: z.array(DiagLine),
});
export type Report = z.infer<typeof Report>;

/// Параметры запуска. Всё необязательное: утилита без параметров ничего отсюда не читает.
export type DiagArgs = {
  domain?: string;
  domains?: string[];
  /// Имена для рукопожатия по SNI.
  hosts?: string[];
  /// До кого мерить наибольший пакет.
  host?: string;
  timeoutMs?: number;
  all?: boolean;
  /// Мерить ли шифрованные точки через стенд — каждая стоит запуска ядра.
  core?: boolean;
};

export const diagTools = () => call(z.array(Tool), "diag_tools");
export const diagRun = (id: string, args?: DiagArgs) => call(Report, "diag_run", { id, args });
/// Сделать то, что утилита предлагает: прописать отмеченное в документ пользователя
/// и довести до живого ядра (D-105). Запуск ничего не меняет, а это — запись.
export const diagApply = (id: string, args?: DiagArgs) => call(Report, "diag_apply", { id, args });

/// Справочник резолверов — тот же, из которого берёт кандидатов `dns-race` (D-097).
export const DnsServer = z.object({ proto: z.string(), addr: z.string(), ipv6: z.boolean() });
export type DnsServer = z.infer<typeof DnsServer>;
export const DnsVariant = z.object({
  id: z.string(),
  name: z.string(),
  filter: z.string(),
  servers: z.array(DnsServer),
});
export type DnsVariant = z.infer<typeof DnsVariant>;
export const DnsProvider = z.object({
  id: z.string(),
  name: z.string(),
  note: z.string(),
  site: z.string(),
  variants: z.array(DnsVariant),
});
export type DnsProvider = z.infer<typeof DnsProvider>;
export const diagProviders = () =>
  call(z.object({ version: z.number(), providers: z.array(DnsProvider) }), "diag_providers");
