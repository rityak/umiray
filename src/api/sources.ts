/// Источники узлов (D-032): подписки, ссылки, файлы.

import { z } from "zod";
import { call } from "./call";

/// Источник — откуда взялись узлы: подписка или «мои ссылки» (D-032).
export const Source = z.object({
  id: z.string(),
  name: z.string(),
  /// Адрес подписки. Пусто у «моих ссылок» — обновлять их неоткуда.
  url: z.string().nullable(),
  /// Секунды эпохи. Форматирует интерфейс, бэкенд времени не знает.
  updated: z.number().nullable(),
  nodes: z.number(),
  /// Источник хранит записи `proxies:`, а не ссылки: узлы в нём написал клиент, и их
  /// можно убрать (D-121). У подписки удаление было бы враньём — узел вернётся.
  records: z.boolean(),
});
export type Source = z.infer<typeof Source>;

/// Результат добавления источника.
export const Import = z.object({
  /// Служебные сообщения провайдера — лимит устройств и подобное.
  notices: z.array(z.string()),
  source: Source,
});
export type Import = z.infer<typeof Import>;

export const sourcesList = () => call(z.array(Source), "sources_list");
export const sourcesAdd = (input: string) => call(Import, "sources_add", { input });
export const sourcesRefresh = (id: string) => call(Import, "sources_refresh", { id });
/// Обновить все подписки разом. Отдаёт строки о том, что **не** получилось: одна упавшая
/// подписка не отменяет остальных.
export const sourcesRefreshAll = () => call(z.array(z.string()), "sources_refresh_all");
/// Отдаёт предупреждение, если на удаляемый источник ссылаются ваши группы или любой набор
/// правил (D-075), иначе `null`. Пересобрать чужой документ клиент не вправе, а ядро на ссылку
/// в никуда отвечает отказом стартовать целиком — молчать об этом нельзя.
export const sourcesDelete = (id: string) => call(z.string().nullable(), "sources_delete", { id });
/// Что прислала панель, слово в слово (D-065). Правится именно это: собранный файл,
/// который читает ядро, пересобирается из него при каждом обновлении.
export const sourcesRead = (id: string) => call(z.string(), "sources_read", { id });
export const sourcesWrite = (id: string, text: string) =>
  call(Import, "sources_write", { id, text });

/// Узел, собранный руками или принесённый файлом (D-120). Запись, а не ссылку: ссылку
/// пришлось бы выдумать, а имена полей записи берутся из документации ядра.
export const sourcesAddProxy = (entry: Record<string, unknown>) =>
  call(Import, "sources_add_proxy", { entry });
export const sourcesAddProxyText = (text: string) =>
  call(Import, "sources_add_proxy_text", { text });
/// Тот же узел текстом — для «кода» в окне сборки. Рендерит бэкенд: второй YAML в окне
/// разошёлся бы с первым.
export const sourcesProxyYaml = (entry: Record<string, unknown>) =>
  call(z.string(), "sources_proxy_yaml", { entry });
/// Системное окно выбора файла и разбор того, что выбрали. `null` — закрыли окно.
export const sourcesAddFile = () => call(Import.nullable(), "sources_add_file");
/// Cloudflare WARP, выпущенный самим клиентом (D-165): регистрация у Cloudflare и узел.
export type WarpTunnel = "masque" | "wireguard";
export const sourcesAddWarp = (tunnel: WarpTunnel) => call(Import, "sources_add_warp", { tunnel });
