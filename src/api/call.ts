/// Граница с бэкендом (D-144): как вызвать команду, проверить ответ и понять ошибку.

import { type InvokeArgs, invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import { t } from "../i18n";

/// Ошибка с бэкенда (D-028). `kind` — машинно-читаемая причина, `details` — строки лога
/// ядра или сообщения провайдера: их показываем списком, а не склеиваем в текст.
export const AppError = z.object({
  kind: z.string(),
  message: z.string(),
  details: z.array(z.string()).catch([]),
});
export type AppError = z.infer<typeof AppError>;

/**
 * Всё, что прилетает из Rust, — недоверенные данные. Команда может отвергнуть промис и
 * обычной строкой (например, если упал сам мост Tauri), поэтому форму проверяем, а не приводим.
 */
export function asAppError(raw: unknown): AppError {
  const parsed = AppError.safeParse(raw);
  if (parsed.success) return parsed.data;
  // Не `String(raw)`: наружу это вываливалось как «Error: …» — чужой текст в русском
  // баннере. Сырую строку кладём в `details`, где ей и место (D-028).
  return {
    kind: "unknown",
    message: t("Internal client error."),
    details: [String(raw)],
  };
}

/// Команда, которая ничего не отдаёт: проверять в ответе нечего.
export const done = z.unknown().transform((): void => undefined);

/**
 * Вызвать команду и **проверить** ответ по схеме: данные из Rust не доверенные,
 * и `invoke<T>` только делал вид, что знает их форму. Поле, переименованное на одной
 * стороне границы, раньше доезжало до окна как `undefined` и ломало его где-то в глубине;
 * теперь команда отказывает сразу, с именем поля в подробностях — тем же путём, что
 * и любая ошибка бэкенда (D-028, D-144). Лишние поля схема отбрасывает: окну они не обещаны.
 */
export async function call<T extends z.ZodType>(
  schema: T,
  command: string,
  args?: InvokeArgs,
): Promise<z.output<T>> {
  const parsed = schema.safeParse(await invoke<unknown>(command, args));
  if (parsed.success) return parsed.data;
  throw {
    kind: "unexpected",
    message: t("Unexpected response from the backend — reinstall the client."),
    details: [command, z.prettifyError(parsed.error)],
  } satisfies AppError;
}

/** Как часто окно опрашивает бэкенд. Одно число на все опросы: разнобой заметен глазом. */
export const POLL_MS = 1500;

/// Версия клиента. Из сборки, а не руками: отчёт диагностики называет её, и разъехаться
/// с настоящей она не должна.
export const VERSION: string = __APP_VERSION__;
