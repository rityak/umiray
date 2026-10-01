/// Windows вокруг клиента: права, автозапуск, kill switch, сброс.

import { z } from "zod";
import { call, done } from "./call";
import { Status } from "./core";

export const systemRelaunchElevated = () => call(done, "system_relaunch_elevated");
/// Сброс всего, кроме скачанного ядра и идентификатора устройства. Ядро останавливает сам.
export const systemReset = () => call(Status, "system_reset");
export const systemAutostartSet = (on: boolean) => call(Status, "system_autostart_set", { on });

/// «Всегда от администратора» (D-087). Отдельная команда, а не поле настроек: это задача
/// в планировщике, и завести её можно только с правами — отказ приезжает как ошибка
/// с кнопкой, а не как молча не сохранившийся тумблер.
export const systemAlwaysAdminSet = (on: boolean) =>
  call(Status, "system_always_admin_set", { on });

/// Отдельная команда, а не поле в `settingsUpdate`: тумблер меняет состояние машины
/// и обязан сработать сейчас, а не при следующем подключении (D-073).
export const systemKillSwitchSet = (on: boolean) => call(Status, "system_kill_switch_set", { on });
/// Идентификатор устройства — справочно, в настройках. Отдельным запросом, а не полем
/// статуса: он не меняется никогда, а статус опрашивается каждую секунду.
export const systemDevice = () => call(z.string(), "system_device");
/// Настройки копией базы через окно сохранения (D-163). Путь к файлу; `null` — окно закрыли.
export const systemExport = () => call(z.string().nullable(), "system_export");
export const systemLanguage = () => call(z.enum(["en", "ru"]), "system_language");
/// Страница на GitHub в браузере по умолчанию. Другие адреса бэкенд не откроет.
export const systemOpenGithub = (url: string) => call(done, "system_open_github", { url });
