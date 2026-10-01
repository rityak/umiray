/// Rule sets (D-157): скачанные списки доменов и подсетей — и geo-базы ядра.

import { z } from "zod";
import { call, done } from "./call";

export const RuleList = z.object({
  /// Имя в правиле: `RULE-SET,<id>,…`.
  id: z.string(),
  title: z.string(),
  titleEn: z.string().nullish(),
  urls: z.array(z.string()),
  /// Когда скачали, секунды эпохи.
  updated: z.number().nullish(),
  /// Когда опубликован (`Last-Modified`): свежесть самого списка, а не нашей загрузки.
  published: z.number().nullish(),
  domains: z.number(),
  cidrs: z.number(),
  skipped: z.number(),
  /// Почему последнее обновление не удалось; прежние данные при этом на месте.
  error: z.string().nullish(),
});
export type RuleList = z.infer<typeof RuleList>;

/// Строка каталога (`collections/lists.yaml`). Добавлена ли она в маршрут, знает черновик.
export const ListOffer = z.object({
  id: z.string(),
  title: z.string(),
  titleEn: z.string().nullish(),
  group: z.string(),
  note: z.string(),
  noteEn: z.string().nullish(),
  urls: z.array(z.string()),
});
export type ListOffer = z.infer<typeof ListOffer>;

/// Скачанное (D-158: кэш под разделы `rule-sets` наборов).
export const listsList = () => call(z.array(RuleList), "lists_list");
export const listsCatalog = () => call(z.array(ListOffer), "lists_catalog");
/// Скачать список для строки маршрута: из каталога или по адресу. Скачанный отдаётся
/// как есть. Скачивание и сборка ядром — секунды на большом списке.
export const listsFetch = (id: string, url?: string) =>
  call(RuleList, "lists_fetch", { id, url: url ?? null });
export const listsAddUrl = (url: string, title: string) =>
  call(RuleList, "lists_add_url", { url, title });
/// Без `id` — все. Отказ одного не отменяет остальных: причина ляжет в его `error`.
export const listsRefresh = (id?: string) => call(done, "lists_refresh", { id: id ?? null });
/// Докачать всё, на что ссылаются наборы, — после записи кода, где список мог появиться
/// строкой, а не кнопкой.
export const listsEnsure = () => call(done, "lists_ensure");

/// Geo-база ядра и когда её меняли.
export const GeoFile = z.object({ name: z.string(), modified: z.number().nullish() });
export type GeoFile = z.infer<typeof GeoFile>;

export const geoFiles = () => call(z.array(GeoFile), "geo_files");
/// Обновляет само ядро — поэтому только работающее.
export const geoUpdate = () => call(z.array(GeoFile), "geo_update");
