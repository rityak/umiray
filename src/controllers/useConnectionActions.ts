import { useCallback, useState } from "react";
import { toast } from "rootik";
import * as api from "../api";
import { t } from "../i18n";
import * as qd from "../qd/api";
import { failure, type Message, notice } from "../shell/Banner";
import type { Job } from "./useJob";

type Deps = {
  status: api.Status;
  setStatus: (status: api.Status) => void;
  /// Удалённый qd возвращает вид к mihomo на диске (D-161) — настройки перечитываются.
  setSettings: (settings: api.Settings) => void;
  /// Ядро, которое показывают разделы и поднимет кнопка (D-154).
  engine: api.Engine;
  setJob: (job: Job | null) => void;
  report: (message: Message | null) => void;
  /// После питания состояние qd тоже меняется — его опрос живёт отдельно.
  afterPower: () => void;
};

/// Подключение как действие человека (D-060, D-154): питание, перезапуск, режим перехвата,
/// установка ядра.
export function useConnectionActions({
  status,
  setStatus,
  setSettings,
  engine,
  setJob,
  report,
  afterPower,
}: Deps) {
  /// Куда пользователь целится, пока идёт запись. Без этого переключатель на мгновение
  /// отскакивал бы обратно: статус ещё прежний, а нажатие уже произошло.
  const [pending, setPending] = useState<api.Choice | null>(null);

  /// Питание. Одна кнопка на оба направления: она же индикатор состояния (D-060).
  /// Включает выбранное ядро, другое при этом гасит клиент (D-154).
  const shownOn = status.active === engine;
  const power = useCallback(async () => {
    setJob("power");
    report(null);
    try {
      setStatus(shownOn ? await api.coreStop() : await api.coreStart());
    } catch (e) {
      report(failure(e));
    } finally {
      setStatus(await api.coreStatus());
      afterPower();
      setJob(null);
    }
  }, [shownOn, afterPower, setJob, report, setStatus]);

  /// Перезапуск: то, что ядро читает на старте, доезжает только так (D-010).
  const restart = useCallback(async () => {
    setJob("power");
    report(null);
    try {
      setStatus(await api.coreRestart());
    } catch (e) {
      report(failure(e));
      setStatus(await api.coreStatus());
    } finally {
      setJob(null);
    }
  }, [setJob, report, setStatus]);

  /// Режим перехвата (D-060). Работающее ядро бэкенд доводит до него сам: TUN —
  /// перезапуском, System и Proxy — реестром; открытые соединения рвутся (D-143).
  /// Отдаёт отказ (или `null`): мастеру его надо показать у себя — баннер лежит
  /// под модальным окном, а шаг без этого шёл дальше, будто режим встал.
  const choose = useCallback(
    async (choice: api.Choice): Promise<unknown> => {
      setJob("mode");
      setPending(choice);
      report(null);
      try {
        const next = await api.modeSet(choice);
        setStatus(next);
        // Про системный прокси молчать нельзя ни в одну сторону (D-047): не прописался —
        // «Подключён» ещё не значит, что трафик идёт; заменили чужой — это чужой VPN,
        // и его поломка выглядела бы нашей виной.
        if (choice === "system" && next.running && !next.systemProxy) {
          report(notice(t("Couldn't set the system proxy — enter the address in your browser.")));
        } else if (choice === "system" && status.foreignProxy !== null) {
          report(
            notice(
              t("System proxy was {proxy} — it's replaced while connected and restored after.", {
                proxy: status.foreignProxy,
              }),
            ),
          );
        }
        return null;
      } catch (e) {
        report(failure(e));
        setStatus(await api.coreStatus());
        return e;
      } finally {
        setJob(null);
        setPending(null);
      }
    },
    [status.foreignProxy, setJob, report, setStatus],
  );

  /// Скачать mihomo — из баннера «ядра нет» и из формы клиента.
  const install = useCallback(async () => {
    setJob("install");
    report(null);
    try {
      const version = await api.coreInstall("mihomo");
      report(notice(t("{engine} {version} downloaded", { engine: "mihomo", version })));
      setStatus(await api.coreStatus());
    } catch (e) {
      report(failure(e));
    } finally {
      setJob(null);
    }
  }, [setJob, report, setStatus]);

  /// Тумблер «qd» (D-161): включить — скачать, выключить — удалить бинарь. Загрузка —
  /// секунды, о которых говорит тост.
  const qdSwitch = useCallback(
    async (on: boolean) => {
      setJob("install");
      report(null);
      const id = on ? toast({ title: t("Downloading qd…"), loading: true }) : undefined;
      try {
        if (on) {
          const version = await api.coreInstall("qd");
          toast.success(t("{engine} {version} downloaded", { engine: "qd", version }), { id });
        } else {
          await qd.remove();
        }
      } catch (e) {
        if (id) toast.dismiss(id);
        report(failure(e));
      } finally {
        setStatus(await api.coreStatus());
        setSettings(await api.settingsGet());
        setJob(null);
      }
    },
    [setJob, report, setStatus, setSettings],
  );

  /// Тумблер маршрутизации (D-166): сборка меняется целиком и доезжает до ядра сразу,
  /// поэтому перечитываем и статус, и настройки, где лежит галка.
  const routing = useCallback(
    async (on: boolean) => {
      report(null);
      try {
        setStatus(await api.routingSet(on));
      } catch (e) {
        report(failure(e));
      } finally {
        setSettings(await api.settingsGet());
      }
    },
    [report, setStatus, setSettings],
  );

  /// Режим, который показывает переключатель: целевой, пока идёт запись, иначе выбранный.
  const mode: api.Choice = pending ?? status.desiredMode;

  return { power, restart, choose, install, qdSwitch, routing, mode };
}
