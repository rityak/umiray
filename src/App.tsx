import { Activity, Layers, Network, Route, Rss, ScrollText, SlidersHorizontal } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { AppShell, Button, Callout, Dialog, Dock, type DockItem } from "rootik";
import type { Choice } from "./api";
import * as api from "./api";
import TitleBar from "./chrome/TitleBar";
import ClientUpdate from "./config/ClientUpdate";
import ConfigEditor from "./config/ConfigEditor";
import { type Drafts, dirty, fromDisk } from "./config/draft";
import Connection from "./connection/Connection";
import Tools from "./diag/Tools";
import { unchanged, usePoll } from "./hooks/usePoll";
import { record } from "./hooks/useTraffic";
import { saveLanguagePreference, t, tk } from "./i18n";
import * as lifecycle from "./lifecycle";
import Logs from "./logs/Logs";
import Settings from "./settings/Settings";
import AdminOffer from "./shell/AdminOffer";
import Banner, { failure, type Message, notice } from "./shell/Banner";
import Debug from "./shell/Debug";
import Sources from "./sources/Sources";

/// Постоянные разделы. Между ними встают разделы конфига: «Группы», «Маршрутизация»,
/// «Настройки» (D-044). Документов внутри может быть больше одного (D-070), но полосе
/// разделов это безразлично. Подписи знает бэкенд, и держать вторую копию списка здесь
/// значило бы разъехаться при первом же переименовании.

/// Значок на раздел. Ключи файлов конфига приходят из `files::list()`, поэтому карта
/// по идентификатору, а не по порядку: переименуют раздел — значок останется на месте.
const ICONS: Record<string, React.ReactNode> = {
  connection: <Network />,
  sources: <Rss />,
  groups: <Layers />,
  rules: <Route />,
  advanced: <SlidersHorizontal />,
  diag: <Activity />,
  logs: <ScrollText />,
};

const FIRST: DockItem[] = [
  { value: "connection", label: tk("Connection"), icon: ICONS.connection },
  { value: "sources", label: tk("Sources"), icon: ICONS.sources },
];
/// Инструменты стоят перед логами и после конфига: сначала настраивают, потом смотрят,
/// что получилось, и только потом читают вывод ядра (D-097). Раздел называется тем,
/// что в нём есть: проверки, которые нужны сами по себе, теперь стоят хуками (D-115).
const LAST: DockItem[] = [
  { value: "diag", label: tk("Tools"), icon: ICONS.diag },
  { value: "logs", label: tk("Logs"), icon: ICONS.logs },
];

/// До первого ответа с диска. `version` тут нулевая намеренно: обратно её никто не шлёт,
/// схему файла знает бэкенд.
const LOADING: api.Settings = {
  version: 0,
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

export default function App() {
  const [status, setStatus] = useState<api.Status>({
    running: false,
    mode: null,
    desiredMode: "local",
    restartReason: null,
    trouble: null,
    port: null,
    corePresent: true,
    elevated: false,
    alwaysAdmin: false,
    systemProxy: false,
    foreignProxy: null,
    autostart: false,
    killSwitch: false,
    started: null,
  });
  const [settings, setSettings] = useState<api.Settings>(LOADING);
  const [sources, setSources] = useState<api.Source[]>([]);
  const [sections, setSections] = useState<api.ConfigSection[]>([]);
  const [message, setMessage] = useState<Message | null>(null);
  /// Чем окно занято, а не просто «занято»: одного флага мало — от него зависит и что
  /// заблокировать (всё три раза одно и то же), и что написать на кнопке, а это разное.
  /// Из-за общего флага сброс подписывался «Скачивание…».
  const [job, setJob] = useState<"power" | "mode" | "install" | "reset" | "update" | null>(null);
  const [updateInfo, setUpdateInfo] = useState<api.UpdateInfo | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updatesOpen, setUpdatesOpen] = useState(false);
  const [formDirty, setFormDirty] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<api.UpdateProgress | null>(null);
  const busy = job !== null;
  useEffect(() => {
    if (job !== "update") return;
    // Removing the confirm button moves focus outside the dialog; capture Escape on the window.
    const preventEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") event.preventDefault();
    };
    window.addEventListener("keydown", preventEscape, true);
    return () => window.removeEventListener("keydown", preventEscape, true);
  }, [job]);
  const [tab, setTab] = useState("connection");
  /// Настройки — не раздел (D-054): они не про то, куда идёт трафик, и место в одном ряду
  /// с «Соединением» занимали зря. Открываются во весь экран под шапкой (D-081).
  const [settingsOpen, setSettingsOpen] = useState(false);
  /// Режим разработчика (D-123). Существует только в dev-сборке: `import.meta.env.DEV`
  /// — константа времени сборки, и в собранном клиенте вся ветка вместе с `DevView`
  /// выбрасывается сборщиком.
  const [dev, setDev] = useState(false);
  /// Кто открыл настройки: `<dialog>` возвращал фокус сам, а обычный экран — нет,
  /// и клавиатура после закрытия оказывалась в `<body>`.
  const opener = useRef<HTMLElement | null>(null);
  /// Нажали «+» в шапке. Кнопка обещает «добавить», значит и довести должна до поля ввода,
  /// а не только до раздела. Флаг гасит сам раздел, иначе фокус уезжал бы туда при каждом
  /// возврате в «Источники».
  const [focusAdd, setFocusAdd] = useState(false);
  /// Куда пользователь целится, пока идёт запись. Без этого переключатель на мгновение
  /// отскакивал бы обратно: статус ещё прежний, а нажатие уже произошло.
  const [pending, setPending] = useState<Choice | null>(null);
  /// Черновики редактора живут здесь, а не в самом редакторе (D-040): уход в другой
  /// раздел размонтирует его, и несохранённый YAML пропал бы вместе с ним.
  const [configs, setConfigs] = useState<Drafts>({});
  // Черновик переживает смену источника и раздела; private не читает raw вовсе (D-138).
  const [sourceDrafts, setSourceDrafts] = useState<Drafts>({});

  // Стабильная ссылка: её держат в зависимостях обработчики разделов.
  const reloadSources = useCallback(() => api.sourcesList().then(setSources, () => {}), []);
  /// Состав документов меняется от действий пользователя: завели набор, применили другой
  /// (D-071). Перечитывает их владелец списка, а не тот раздел, который нажал.
  const reloadSections = useCallback(() => {
    api.configList().then(unchanged(setSections), () => {});
  }, []);

  /// Запуск — список хуков, а не ветка в этом файле (D-095). Окно не знает, из чего он
  /// состоит: оно даёт хукам, куда положить добытое, и ждёт, пока они снимут заставку.
  /// Порядок шагов и их сообщения живут в `lifecycle.ts`.
  useEffect(() => {
    lifecycle.boot({
      settings: setSettings,
      sections: setSections,
      sources: setSources,
      status: setStatus,
      message: setMessage,
      updates: setUpdateInfo,
    });
  }, []);

  // Статус приходит каждый такт, но меняется редко: одинаковый ответ окно не перерисовывает.
  usePoll(() => api.coreStatus().then(unchanged(setStatus), () => {}));

  const configOpen = sections.some((section) => section.id === tab);
  usePoll(reloadSections, configOpen);

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
    setMessage(
      notice(
        t(
          "System proxy is already set to {proxy}. Switch to System to take over, or check which app configured it.",
          { proxy: status.foreignProxy },
        ),
      ),
    );
  }, [status.foreignProxy]);

  /// Одна команда на все настройки (D-037). Показываем выбор сразу, но если запись
  /// не удалась — возвращаем то, что лежит на диске: окно не должно врать.
  const update = useCallback(async (patch: api.SettingsPatch) => {
    setSettings((current) => ({ ...current, ...patch }));
    try {
      setSettings(await api.settingsUpdate(patch));
    } catch (e) {
      setSettings(await api.settingsGet());
      setMessage(failure(e));
    }
  }, []);

  const onDraft = useCallback((id: string, text: string) => {
    setConfigs((current) => ({ ...current, [id]: { ...current[id], text } }));
  }, []);

  /// Файл прочитан с диска — правила слияния с черновиком живут в `fromDisk`.
  const onDisk = useCallback((id: string, text: string) => {
    setConfigs((current) => ({ ...current, [id]: fromDisk(current[id], text) }));
  }, []);

  const onSourceDraft = useCallback((id: string, text: string) => {
    setSourceDrafts((current) => ({ ...current, [id]: { ...current[id], text } }));
  }, []);

  const onSourceDisk = useCallback((id: string, text: string) => {
    setSourceDrafts((current) => ({ ...current, [id]: fromDisk(current[id], text) }));
  }, []);

  const onFocused = useCallback(() => setFocusAdd(false), []);

  /// «Добавить источник» — из шапки и из пустого состояния узлов. Кнопка обещает
  /// «добавить», значит и доводит до поля ввода, а не только до раздела.
  const add = useCallback(() => {
    setTab("sources");
    setFocusAdd(true);
  }, []);

  const install = useCallback(async () => {
    setJob("install");
    setMessage(null);
    try {
      setMessage(notice(await api.coreInstall()));
      setStatus(await api.coreStatus());
    } catch (e) {
      setMessage(failure(e));
    } finally {
      setJob(null);
    }
  }, []);

  const autostart = useCallback(async (on: boolean) => {
    setMessage(null);
    try {
      setStatus(await api.systemAutostartSet(on));
    } catch (e) {
      setMessage(failure(e));
    }
  }, []);

  /// «Всегда от администратора» — заведение или снятие задачи в планировщике (D-087).
  /// Оба действия требуют прав, поэтому отказ приезжает `NeedsElevation` — у баннера
  /// на него уже есть кнопка.
  const alwaysAdmin = useCallback(async (on: boolean) => {
    setMessage(null);
    try {
      setStatus(await api.systemAlwaysAdminSet(on));
      // Предлагать больше нечего: решение принято в любую сторону.
      setSettings(await api.settingsUpdate({ adminOffer: false }));
      if (on) {
        setMessage(
          notice(
            t(
              "The client will launch as administrator without a UAC prompt. The task is named umiray in Task Scheduler.",
            ),
          ),
        );
      }
    } catch (e) {
      setMessage(failure(e));
      setStatus(await api.coreStatus());
    }
  }, []);

  /// Kill switch меняет и настройку, и брандмауэр, поэтому перечитываем оба: галка живёт
  /// в настройках (намерение), а «в силе ли» приходит в статусе (D-073).
  const killSwitch = useCallback(async (on: boolean) => {
    setMessage(null);
    try {
      setStatus(await api.systemKillSwitchSet(on));
      setSettings(await api.settingsGet());
    } catch (e) {
      setMessage(failure(e));
    }
  }, []);

  /// Сброс останавливает ядро и стирает источники, поэтому после него перечитываем всё:
  /// в окне не должно остаться ничего от прошлой жизни.
  const reset = useCallback(async () => {
    setJob("reset");
    setMessage(null);
    try {
      setStatus(await api.systemReset());
      setSettings(await api.settingsGet());
      setConfigs({});
      setSourceDrafts({});
      await reloadSources();
      setMessage(notice(t("Defaults restored. The core and device identifier were kept.")));
    } catch (e) {
      setMessage(failure(e));
    } finally {
      setJob(null);
    }
  }, [reloadSources]);

  const elevate = useCallback(async () => {
    setMessage(null);
    try {
      await api.systemRelaunchElevated();
    } catch (e) {
      setMessage(failure(e));
    }
  }, []);

  /// Режим перехвата (D-060). Работающее ядро бэкенд доводит до него сам: TUN —
  /// перезапуском, System и Proxy — реестром; открытые соединения рвутся (D-143).
  const choose = useCallback(
    async (choice: Choice) => {
      setJob("mode");
      setPending(choice);
      setMessage(null);
      try {
        const next = await api.modeSet(choice);
        setStatus(next);
        // Про системный прокси молчать нельзя ни в одну сторону (D-047): не прописался —
        // «Подключено» ещё не значит, что трафик идёт; заменили чужой — это чужой VPN,
        // и его поломка выглядела бы нашей виной.
        if (choice === "system" && next.running && !next.systemProxy) {
          setMessage(
            notice(
              t("Could not set the Windows proxy — enter the address in your browser manually."),
            ),
          );
        } else if (choice === "system" && status.foreignProxy !== null) {
          setMessage(
            notice(
              t(
                "System proxy was {proxy} — it will be replaced on connection and restored on disconnect.",
                { proxy: status.foreignProxy },
              ),
            ),
          );
        }
      } catch (e) {
        setMessage(failure(e));
        setStatus(await api.coreStatus());
      } finally {
        setJob(null);
        setPending(null);
      }
    },
    [status.foreignProxy],
  );

  /// Питание. Одна кнопка на оба направления: она же индикатор состояния (D-060).
  const power = useCallback(async () => {
    setJob("power");
    setMessage(null);
    try {
      setStatus(status.running ? await api.coreStop() : await api.coreStart());
    } catch (e) {
      setMessage(failure(e));
      setStatus(await api.coreStatus());
    } finally {
      setJob(null);
    }
  }, [status.running]);

  /// Перезапуск: то, что ядро читает на старте, доезжает только так (D-010).
  const restart = useCallback(async () => {
    setJob("power");
    setMessage(null);
    try {
      setStatus(await api.coreRestart());
    } catch (e) {
      setMessage(failure(e));
      setStatus(await api.coreStatus());
    } finally {
      setJob(null);
    }
  }, []);

  const mode: Choice = pending ?? status.desiredMode;
  const checkUpdate = async () => {
    setCheckingUpdate(true);
    try {
      setUpdateInfo(await api.updatesCheck());
    } catch (error) {
      setMessage(failure(error));
    } finally {
      setCheckingUpdate(false);
    }
  };
  const installUpdate = async () => {
    if (busy) return;
    if (
      formDirty ||
      Object.values(configs).some(dirty) ||
      Object.values(sourceDrafts).some(dirty)
    ) {
      setMessage(notice(t("Save or discard your edits before updating the client.")));
      return;
    }
    setJob("update");
    setMessage(null);
    setUpdateProgress({ phase: "download", downloaded: 0, total: null });
    try {
      await api.updatesInstall(setUpdateProgress);
    } catch (error) {
      setMessage(failure(error));
    } finally {
      setJob(null);
      setUpdateProgress(null);
      api.coreStatus().then(setStatus, () => {});
    }
  };
  /// Сообщение о нужном перезапуске принадлежит состоянию, а не событию: оно висит,
  /// пока расхождение есть, и пропадает само, когда его не станет. Своё сообщение
  /// пользователя при этом главнее — его вызвали только что.
  /// Порядок один и он же приоритет: своё сообщение пользователя главнее всего — его
  /// вызвали только что; дальше сторож (трафик не идёт — это уже поломка), и только
  /// потом предложение перезапустить.
  const banner: Message | null =
    message ??
    (status.trouble
      ? {
          tone: "error" as const,
          text: status.trouble,
          details: [],
          kind: "restartNeeded",
        }
      : null) ??
    (status.restartReason
      ? {
          tone: "info",
          text: status.restartReason,
          details: [],
          kind: "restartNeeded",
        }
      : null);
  const tabs: DockItem[] = [
    ...FIRST.map((item) => ({ ...item, label: t(item.label) })),
    ...sections.map((section) => ({
      value: section.id,
      label: section.label,
      icon: ICONS[section.id],
      // Несохранённое в **любом** документе раздела: точка на вкладке отвечает за раздел
      // целиком, иначе правка в «Клиенте» была бы не видна из «Соединения».
      badge: section.docs.some(
        (doc) => configs[doc.id] !== undefined && configs[doc.id].text !== configs[doc.id].saved,
      ),
      badgeLabel: t("unsaved changes"),
    })),
    ...LAST.map((item) => ({ ...item, label: t(item.label) })),
  ];
  /// Dock виден только над разделами: настройки и отладка занимают окно целиком.
  const section = sections.find((item) => item.id === tab);
  /// Переход в раздел закрывает оформление и отладку: они занимают место раздела.
  const navigate = (id: string) => {
    setSettingsOpen(false);
    setDev(false);
    setTab(id);
    // Сообщение принадлежит разделу, который его вызвал: в чужом оно уже враньё.
    // Детали со стороны сохраняем — ради них как раз и переходят в другой раздел.
    setMessage((current) => (current?.details.length ? current : null));
  };
  const current = settingsOpen || dev ? null : tab;

  return (
    // Раскладку (islands / inset), материал и тему решает оформление rootik (D-142).
    <AppShell
      className="um-shell"
      headerShape="none"
      dimWhenInactive
      header={
        <TitleBar
          status={status}
          powering={job === "power"}
          onAdd={add}
          onSettings={() => {
            opener.current = document.activeElement as HTMLElement | null;
            setSettingsOpen(true);
          }}
          dev={dev}
          onDev={import.meta.env.DEV ? () => setDev((was) => !was) : undefined}
        />
      }
      dock={
        <Dock
          aria-label={t("Sections")}
          mode="tabs"
          variant="labels"
          items={tabs.map((item) => ({
            ...item,
            id: `tab-${item.value}`,
            controls: "section-panel",
          }))}
          value={current ?? undefined}
          onChange={navigate}
        />
      }
    >
      {(updatesOpen || updateProgress) && (
        <Dialog
          title={t("Client updates")}
          onClose={() => {
            if (!updateProgress) setUpdatesOpen(false);
          }}
          dismissible={!updateProgress}
          hideClose={updateProgress !== null}
        >
          <ClientUpdate
            info={updateInfo}
            checking={checkingUpdate}
            progress={updateProgress}
            busy={busy}
            onCheck={checkUpdate}
            onInstall={installUpdate}
          />
        </Dialog>
      )}
      {/* Один столбец с зазором: сообщения и раздел — отдельные блоки, а не впритык. */}
      {/* Раздел занимает ровно окно между шапкой и dock и прокручивает свои карточки сам:
          страница не прокручивается никогда, и dock ничего не перекрывает (STYLEGUIDE).
          `pb-1.5`: отступ набора под dock на 6 px меньше зазора между карточками. */}
      <div className="flex min-h-0 flex-1 flex-col gap-3 pb-1.5">
        {/* Предложение закрепить права. Всплывает ровно тогда, когда оно уместно:
              клиент запущен с правами, задачи ещё нет и от неё ещё не отказывались
              (D-087). Отказ помнится — предложение, возвращающееся каждый запуск,
              это уже не предложение. */}
        {status.elevated && !status.alwaysAdmin && settings.adminOffer && (
          <AdminOffer
            onAccept={() => alwaysAdmin(true)}
            onDismiss={() => update({ adminOffer: false })}
          />
        )}

        <Banner
          message={banner}
          onInstall={install}
          onElevate={elevate}
          onRestart={restart}
          onDismiss={message ? () => setMessage(null) : undefined}
        />

        {updateInfo?.version && job !== "update" && (
          <Callout
            tone="info"
            title={t("umiray {version} is available", { version: updateInfo.version })}
            actions={
              <Button size="sm" onClick={() => setUpdatesOpen(true)}>
                {t("Client updates")}
              </Button>
            }
          />
        )}

        {/* Оформление и отладка занимают место раздела целиком; шапка и dock остаются. */}
        {dev ? (
          <Debug />
        ) : settingsOpen ? (
          <Settings
            onClose={() => {
              setSettingsOpen(false);
              opener.current?.focus();
            }}
          />
        ) : (
          <>
            {/* Подложку несёт каждая карточка раздела сама (D-142): листа под разделом нет. */}
            <section
              id="section-panel"
              role="tabpanel"
              aria-labelledby={`tab-${tab}`}
              className="flex min-h-0 flex-1 flex-col gap-3"
            >
              {tab === "connection" && (
                <Connection
                  status={status}
                  mode={mode}
                  busy={busy}
                  powering={job === "power"}
                  onMode={choose}
                  onPower={power}
                  sources={sources}
                  hidden={settings.private}
                  onHidden={() => update({ private: !settings.private })}
                  onStatus={setStatus}
                  onAdd={add}
                  onMessage={setMessage}
                />
              )}
              {tab === "sources" && (
                <Sources
                  sources={sources}
                  hidden={settings.private}
                  onHidden={() => update({ private: !settings.private })}
                  focus={focusAdd}
                  onFocused={onFocused}
                  schedule={settings.refresh}
                  onSchedule={(refresh) => update({ refresh })}
                  drafts={sourceDrafts}
                  onDraft={onSourceDraft}
                  onDisk={onSourceDisk}
                  onChanged={reloadSources}
                  onMessage={setMessage}
                />
              )}
              {section && (
                <ConfigEditor
                  section={section}
                  drafts={configs}
                  onDraft={onDraft}
                  onDisk={onDisk}
                  onSections={reloadSections}
                  onMessage={setMessage}
                  client={{
                    settings,
                    status,
                    busy,
                    installing: job === "install",
                    updateInfo,
                    checkingUpdate,
                    updateProgress,
                    onCheckUpdate: checkUpdate,
                    onClientUpdate: installUpdate,
                    onUnsavedChange: setFormDirty,
                    onChange: update,
                    onInstall: install,
                    onAutostart: autostart,
                    onAlwaysAdmin: alwaysAdmin,
                    onKillSwitch: killSwitch,
                    onReset: reset,
                    onStatus: setStatus,
                    onLanguageChange: (language) => {
                      if (Object.values(configs).some((draft) => draft.text !== draft.saved)) {
                        setMessage(
                          notice(t("Save or discard your edits before changing the language.")),
                        );
                        return;
                      }
                      try {
                        saveLanguagePreference(language);
                        window.location.reload();
                      } catch (error) {
                        setMessage(failure(error));
                      }
                    },
                  }}
                />
              )}
              {tab === "diag" && <Tools onMessage={setMessage} />}
              {tab === "logs" && <Logs hidden={settings.private} />}
            </section>
          </>
        )}
      </div>
    </AppShell>
  );
}
