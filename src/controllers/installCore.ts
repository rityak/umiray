import * as api from "../api";
import { t } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";

/// Скачать выбранную сборку mihomo (D-194) и сказать, чем всё кончилось: из заставки
/// первого запуска и из кнопки «Обновить». Статус после — всегда: сборка, которой ядро
/// будет работать, видна только по нему.
///
/// Выбранная не встала, но запасная есть — это не «ядра нет», а предупреждение: VPN
/// работает, просто другой сборкой.
export async function installCore(): Promise<{ status: api.Status; message: Message }> {
  try {
    const version = await api.coreInstall("mihomo");
    const status = await api.coreStatus();
    if (status.coreInUse !== status.coreBuild) {
      return {
        status,
        message: notice(
          t("{build} didn't install — {engine} {version} downloaded instead", {
            build: status.coreBuild,
            engine: status.coreInUse,
            version,
          }),
        ),
      };
    }
    return {
      status,
      message: notice(t("{engine} {version} downloaded", { engine: status.coreInUse, version })),
    };
  } catch (e) {
    const status = await api.coreStatus();
    if (status.corePresent) {
      const error = api.asAppError(e);
      return {
        status,
        message: notice(
          t("Couldn't download {build} — {engine} is used", {
            build: status.coreBuild,
            engine: status.coreInUse,
          }),
          [error.message, ...error.details],
        ),
      };
    }
    return { status, message: failure(e) };
  }
}
