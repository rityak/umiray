import { useEffect, useRef, useState } from "react";
import * as api from "../api";
import { unchanged, usePoll } from "../hooks/usePoll";
import { record } from "../hooks/useTraffic";
import { t } from "../i18n";
import { type Message, notice } from "../shell/Banner";

/// До первого ответа бэкенда: ничего не работает, ядро на месте.
const INITIAL: api.Status = {
  active: null,
  running: false,
  mode: null,
  desiredMode: "local",
  restartReason: null,
  trouble: null,
  port: null,
  corePresent: true,
  qdPresent: false,
  elevated: false,
  alwaysAdmin: false,
  systemProxy: false,
  foreignProxy: null,
  autostart: false,
  killSwitch: false,
  started: null,
};

/// Статус как есть (D-155): что работает, в каком режиме, что вокруг — и трафик, пока
/// работает mihomo. Владеет опросом статуса и счётчиком трафика.
export function useStatus(report: (message: Message) => void) {
  const [status, setStatus] = useState<api.Status>(INITIAL);

  // Статус приходит каждый такт, но меняется редко: одинаковый ответ окно не перерисовывает.
  usePoll(() => api.coreStatus().then(unchanged(setStatus), () => {}));

  /// Трафик опрашиваем всё время, пока ядро работает, а не только в открытом разделе:
  /// обнулять историю графика при уходе в «Логи» и обратно значит врать про прошедшие
  /// полминуты (D-076). Отсчёты уходят в хранилище, а не в состояние окна, — иначе
  /// каждый такт перерисовывал бы всё окно, а не только «Соединение».
  usePoll(() => {
    api.coreTraffic().then(record, () => {});
  }, status.running);

  useEffect(() => {
    if (!status.running) record(null);
  }, [status.running]);

  /// Чужой системный прокси при запуске (D-115): трафик машины уже куда-то идёт, и об этом
  /// говорят один раз. Именно один: запись в реестре — состояние, и висящий из-за неё
  /// баннер перекрывал бы всё остальное, пока у человека работает второй VPN.
  const told = useRef(false);
  useEffect(() => {
    if (told.current || status.foreignProxy === null) return;
    told.current = true;
    report(
      notice(
        t(
          "System proxy is already set to {proxy}. Switch to System to replace it, or find the app that set it.",
          { proxy: status.foreignProxy },
        ),
      ),
    );
  }, [status.foreignProxy, report]);

  return { status, setStatus };
}
