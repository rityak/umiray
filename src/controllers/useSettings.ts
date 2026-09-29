import { useCallback, useState } from "react";
import * as api from "../api";
import { failure, type Message } from "../shell/Banner";

/// До первого ответа с диска. `version` тут нулевая намеренно: обратно её никто не шлёт,
/// схему файла знает бэкенд.
const LOADING: api.Settings = {
  version: 0,
  engine: "mihomo",
  refresh: { onStart: true, everyMinutes: 1440 },
  theme: "midnight",
  scene: true,
  sceneBlur: 3,
  effects: true,
  private: false,
  killSwitch: false,
  autoConnect: false,
  launch: "smart",
  // Пока настройки не приехали, предложение не показываем: иначе оно мигало бы
  // на долю секунды у тех, кто от него уже отказался.
  adminOffer: false,
};

/// Настройки клиента (D-155): одна команда на все поля (D-037).
export function useSettings(report: (message: Message) => void) {
  const [settings, setSettings] = useState<api.Settings>(LOADING);

  /// Показываем выбор сразу, но если запись не удалась — возвращаем то, что лежит на диске:
  /// окно не должно врать.
  const update = useCallback(
    async (patch: api.SettingsPatch) => {
      setSettings((current) => ({ ...current, ...patch }));
      try {
        setSettings(await api.settingsUpdate(patch));
      } catch (e) {
        setSettings(await api.settingsGet());
        report(failure(e));
      }
    },
    [report],
  );

  return { settings, setSettings, update };
}
