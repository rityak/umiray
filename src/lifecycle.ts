/**
 * Хуки жизненного цикла клиента (D-095).
 *
 * Запуск — это список шагов, а не одна функция в `App`: каждый шаг знает своё имя,
 * своё сообщение для заставки и свою неудачу. Окно при этом не знает **ни одного**
 * из них — оно даёт хукам, куда положить результат, и ждёт.
 *
 * Зачем не просто `useEffect`: шагов будет больше (миграции, восстановление сессии,
 * проверка обновлений), и каждый новый — это одна запись здесь, а не ещё одна ветка
 * в эффекте на двести строк. Порядок виден списком, а не порядком `await`.
 *
 * Фаза сейчас одна — запуск. Сами хуки лежат внизу этого же файла: их два, и заводить
 * им по файлу рано. Появится третий-четвёртый — переедут в `lifecycle/`, регистрация
 * останется той же.
 *
 * Потолок: у фазы нет имени, потому что фаза одна. Второй фазе («перед подключением»,
 * «после смены настроек») понадобится свой контекст — тогда `on` получит первым
 * аргументом имя фазы, а `Boot` станет одним из нескольких контекстов. Заводить эту
 * развилку сейчас — писать обобщение под единственного потребителя.
 */

import * as api from "./api";
import { t, tk } from "./i18n";
import { failure, type Message, notice } from "./shell/Banner";
import * as splash from "./splash";

/// Куда хук кладёт добытое. Это ровно те состояния окна, которые запуск и наполняет, —
/// а не `App` целиком: хук, которому дали окно, рано или поздно начнёт его перерисовывать.
export type Boot = {
  settings: (settings: api.Settings) => void;
  sections: (sections: api.ConfigSection[]) => void;
  sources: (sources: api.Source[]) => void;
  status: (status: api.Status) => void;
  message: (message: Message) => void;
  updates: (info: api.UpdateInfo) => void;
};

export type Hook = {
  /// Кем шаг назван в логе и в сообщении об отказе. Он же не даёт зарегистрировать
  /// себя дважды: при горячей перезагрузке модуль выполняется повторно.
  id: string;
  /// Что показать в заставке, пока шаг идёт. Шаг, который **по ходу** узнал, что будет
  /// долгим, скажет об этом сам — `splash.step` и `splash.hold` открыты всем.
  label: string;
  run: (boot: Boot) => Promise<void>;
};

const startup: Hook[] = [];

/// Добавить шаг запуска. Порядок — порядок вызовов: следующий начинается, когда
/// предыдущий закончился, потому что настройки нужны раньше, чем то, что от них зависит.
export function on(hook: Hook): void {
  const at = startup.findIndex((existing) => existing.id === hook.id);
  if (at === -1) startup.push(hook);
  else startup[at] = hook;
}

/// Пройти все шаги и снять заставку. Заставку снимает именно этот вызов, а не монтирование
/// окна: смонтированная пустая оболочка — та же неготовность, только другого цвета.
///
/// Упавший шаг не уносит остальные: окно обязано открыться в любом случае, а причина
/// уезжает баннером. Свою неудачу шаг объясняет лучше — этот перехват для того, чего
/// он не предвидел.
export async function boot(ctx: Boot): Promise<void> {
  try {
    for (const hook of startup) {
      splash.step(t(hook.label));
      try {
        await hook.run(ctx);
      } catch (e) {
        ctx.message(failure(e));
      }
    }
  } finally {
    splash.done();
  }
}

/// Из чего состоит окно: тема и настройки, список документов, источники. Три ответа
/// независимы, поэтому идут разом; упавший из них говорит за себя и не отменяет соседей.
on({
  id: "state",
  label: tk("settings and sources"),
  async run(boot) {
    // Настройки живут на диске (D-024), не в localStorage: без этого ни выбор
    // не переживал бы перезапуск, ни автообновление не было бы возможно в принципе.
    await Promise.allSettled([
      api.settingsGet().then(boot.settings, (e) => boot.message(failure(e))),
      api.configList().then(boot.sections, (e) => boot.message(failure(e))),
      api.sourcesList().then(boot.sources, (e) => boot.message(failure(e))),
    ]);
  },
});

/// Донести ядро, если его нет (D-094).
///
/// **Только когда его нет.** Обновлять само, без спроса, клиент не берётся: политика
/// обновления ядра ещё не решена (DECISION, «Не решено»), а подменять рабочий бинарь
/// под работающим VPN — это отдельное решение, а не побочный эффект запуска.
///
/// Ждём под заставкой, а не в открытом окне. Случай ровно один — первый запуск,
/// и делать в окне тогда всё равно нечего: ни источников, ни ядра.
on({
  id: "core",
  label: tk("mihomo core"),
  async run(boot) {
    const status = await api.coreStatus().catch(() => null);
    if (status === null) return;
    boot.status(status);
    if (status.corePresent) return;

    // Строка называет и то, чего ждём, и то, что это не повторится: без второй половины
    // полминуты на бегунке читаются как «повисло». Потолок заставки на это время поднят.
    splash.step(t("downloading mihomo — once on first launch"));
    splash.hold();
    try {
      boot.message(notice(await api.coreInstall()));
      boot.status(await api.coreStatus());
    } catch (e) {
      // Окно откроется без ядра — и скажет об этом кнопкой, а не только текстом:
      // причина уезжает в подробности, а действие остаётся тем же (D-028).
      const error = api.asAppError(e);
      boot.message({
        tone: "error",
        text: t("The core could not be downloaded automatically — VPN cannot start without it."),
        details: [error.message, ...error.details],
        kind: "coreMissing",
      });
    }
  },
});

on({
  id: "updates",
  label: tk("Client updates"),
  async run(boot) {
    // A slow or unavailable release server must not delay opening the window.
    void api.updatesCheck().then(boot.updates, () => {});
  },
});
