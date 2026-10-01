import { useCallback } from "react";
import * as api from "../api";
import { t } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";
import type { Job } from "./useJob";

type Deps = {
  setStatus: (status: api.Status) => void;
  setSettings: (settings: api.Settings) => void;
  setJob: (job: Job | null) => void;
  report: (message: Message | null) => void;
  /// Сброс стирает источники и черновики — окно перечитывает всё.
  afterReset: () => Promise<void>;
};

/// Windows вокруг клиента как действие человека: автозапуск, «всегда от администратора»,
/// kill switch, сброс, перезапуск с правами.
export function useSystemActions({ setStatus, setSettings, setJob, report, afterReset }: Deps) {
  const autostart = useCallback(
    async (on: boolean) => {
      report(null);
      try {
        setStatus(await api.systemAutostartSet(on));
      } catch (e) {
        report(failure(e));
      }
    },
    [report, setStatus],
  );

  /// «Всегда от администратора» — заведение или снятие задачи в планировщике (D-087).
  /// Оба действия требуют прав, поэтому отказ приезжает `NeedsElevation` — у баннера
  /// на него уже есть кнопка.
  const alwaysAdmin = useCallback(
    async (on: boolean) => {
      report(null);
      try {
        setStatus(await api.systemAlwaysAdminSet(on));
        // Предлагать больше нечего: решение принято в любую сторону.
        setSettings(await api.settingsUpdate({ adminOffer: false }));
        if (on) {
          report(
            notice(
              t(
                "The client will start as administrator with no UAC prompt. The task is called umiray in Task Scheduler.",
              ),
            ),
          );
        }
      } catch (e) {
        report(failure(e));
        setStatus(await api.coreStatus());
      }
    },
    [report, setStatus, setSettings],
  );

  /// Kill switch меняет и настройку, и брандмауэр, поэтому перечитываем оба: галка живёт
  /// в настройках (намерение), а «в силе ли» приходит в статусе (D-073).
  const killSwitch = useCallback(
    async (on: boolean) => {
      report(null);
      try {
        setStatus(await api.systemKillSwitchSet(on));
        setSettings(await api.settingsGet());
      } catch (e) {
        report(failure(e));
      }
    },
    [report, setStatus, setSettings],
  );

  /// Сброс останавливает ядра и стирает источники, поэтому после него перечитываем всё:
  /// в окне не должно остаться ничего от прошлой жизни.
  const reset = useCallback(async () => {
    setJob("reset");
    report(null);
    try {
      setStatus(await api.systemReset());
      setSettings(await api.settingsGet());
      await afterReset();
      report(notice(t("Settings reset. The core and device ID were kept.")));
    } catch (e) {
      report(failure(e));
    } finally {
      setJob(null);
    }
  }, [setJob, report, setStatus, setSettings, afterReset]);

  const elevate = useCallback(async () => {
    report(null);
    try {
      await api.systemRelaunchElevated();
    } catch (e) {
      report(failure(e));
    }
  }, [report]);

  /// Настройки архивом (D-163). Закрыли окно сохранения — сказать нечего.
  const exportSettings = useCallback(async () => {
    report(null);
    try {
      const path = await api.systemExport();
      if (path) report(notice(t("Settings saved to {path}", { path })));
    } catch (e) {
      report(failure(e));
    }
  }, [report]);

  return { autostart, alwaysAdmin, killSwitch, reset, elevate, exportSettings };
}
