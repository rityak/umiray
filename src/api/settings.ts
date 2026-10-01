/// Настройки клиента (D-024): поведение, живущее на диске.

import { z } from "zod";
import { tk } from "../i18n";
import { call } from "./call";
import { Engine } from "./core";

/// Тема прежнего оформления. Бэкенд её хранит, окно не читает: вид теперь у rootik (D-142).
export const Theme = z.enum(["midnight", "green", "purple"]);
export type Theme = z.infer<typeof Theme>;

/// Настройки: поведение, живущее на диске (D-024). `version` — схема файла, фронтенд её
/// не трогает: поля меняет бэкенд отдельными командами.
/// Когда обновлять подписки. Два поля, а не одно: «только при запуске» — это поведение,
/// а не период.
export const Refresh = z.object({
  onStart: z.boolean(),
  /// Ноль — по времени не обновлять.
  everyMinutes: z.number(),
});
export type Refresh = z.infer<typeof Refresh>;

/// Как открывается окно при запуске (D-088, D-129).
export const Launch = z.enum(["smart", "window", "tray"]);
export type Launch = z.infer<typeof Launch>;

export const Settings = z.object({
  version: z.number(),
  engine: Engine.catch("mihomo"),
  refresh: Refresh,
  theme: Theme,
  // Намерение «прописывать ли прокси в систему» бэкенд тоже хранит, но окну оно не нужно
  // и здесь не объявлено намеренно: рядом живёт `Status.systemProxy` — **факт из реестра**,
  // и два поля с одним именем в одном файле читались как одно и то же (см. карту, §7.6).
  // Окно спрашивает у реестра, а не у намерения: если запись не удалась, показать надо правду.
  /// Пиксельная сцена фоном (D-093). Отдельно от дождя: картинка — это вид, движение —
  /// это дождь, и гасят их по разным поводам.
  scene: z.boolean(),
  /// Размытие сцены, пиксели (D-132). Дождь не трогает.
  sceneBlur: z.number(),
  /// Дождь (D-045). Системный `prefers-reduced-motion` гасит его поверх этого флага:
  /// настройка означает «пользователь так захотел», а не «так решила система».
  effects: z.boolean(),
  /// Режим «на людях»: адреса серверов и имена подписок закрыты точками, чтобы клиент
  /// можно было показать на экране, не выдав, куда человек подключён.
  private: z.boolean(),
  /// Запирать ли выход мимо туннеля, пока работает TUN (D-073). Здесь **намерение** —
  /// в отличие от системного прокси, у которого окну нужен только факт: тумблер может
  /// стоять и не быть в силе, и об этом надо сказать словами, а не снимать галку.
  killSwitch: z.boolean(),
  /// Подключаться сразу при запуске клиента (D-088). Что поднимать, помнить не нужно:
  /// направление и режим перехвата переживают перезапуск сами.
  autoConnect: z.boolean(),
  /// Показывать ли окно при запуске (D-088).
  launch: Launch,
  /// Предлагать ли «всегда от администратора», когда клиент запущен с правами, а задачи
  /// ещё нет (D-087). Отказ помнится: предложение, возвращающееся каждый запуск, —
  /// это уже не предложение.
  adminOffer: z.boolean(),
  /// Пройден ли мастер первого запуска (D-162). Закрытый — тоже пройден.
  setup: z.boolean(),
  /// Работает ли маршрутизация (D-166). Меняется своей командой, не `SettingsPatch`:
  /// переключение доезжает до ядра.
  routing: z.boolean(),
});
export type Settings = z.infer<typeof Settings>;

/// Что меняем в настройках. Присылаем только изменившееся — остальное бэкенд не тронет.
export type SettingsPatch = {
  engine?: Engine;
  refresh?: Refresh;
  theme?: Theme;
  scene?: boolean;
  sceneBlur?: number;
  effects?: boolean;
  private?: boolean;
  autoConnect?: boolean;
  launch?: Launch;
  adminOffer?: boolean;
  setup?: boolean;
};

/// Готовые варианты для выпадающего списка. «Своё» — не пресет, его считает форма.
export const REFRESH_PRESETS: { label: string; value: Refresh }[] = [
  { label: tk("Never refresh"), value: { onStart: false, everyMinutes: 0 } },
  { label: tk("Only on startup"), value: { onStart: true, everyMinutes: 0 } },
  { label: tk("Every hour"), value: { onStart: true, everyMinutes: 60 } },
  { label: tk("Every 6 hours"), value: { onStart: true, everyMinutes: 360 } },
  { label: tk("Every day"), value: { onStart: true, everyMinutes: 1440 } },
];

/** Совпадает ли текущая настройка с готовым вариантом. Нет — значит выбрано «Своё». */
export function refreshPreset(refresh: Refresh): number {
  return REFRESH_PRESETS.findIndex(
    (preset) =>
      preset.value.onStart === refresh.onStart &&
      preset.value.everyMinutes === refresh.everyMinutes,
  );
}

export const settingsGet = () => call(Settings, "settings_get");
export const settingsUpdate = (patch: SettingsPatch) =>
  call(Settings, "settings_update", { patch });
