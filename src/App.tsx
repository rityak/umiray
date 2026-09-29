import { Activity, Layers, Network, Route, Rss, ScrollText, SlidersHorizontal } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { AppShell, Button, Callout, Dialog, Dock, type DockItem } from "rootik";
import type * as api from "./api";
import TitleBar from "./chrome/TitleBar";
import ClientUpdate from "./config/ClientUpdate";
import ConfigEditor from "./config/ConfigEditor";
import Connection from "./connection/Connection";
import { useConnectionActions } from "./controllers/useConnectionActions";
import { useDrafts } from "./controllers/useDrafts";
import { useJob } from "./controllers/useJob";
import { useSections } from "./controllers/useSections";
import { useSettings } from "./controllers/useSettings";
import { useSources } from "./controllers/useSources";
import { useStatus } from "./controllers/useStatus";
import { useSystemActions } from "./controllers/useSystemActions";
import { useUpdates } from "./controllers/useUpdates";
import Tools from "./diag/Tools";
import { ENGINES } from "./engines";
import { saveLanguagePreference, t, tk } from "./i18n";
import * as lifecycle from "./lifecycle";
import Logs from "./logs/Logs";
import QdSection from "./qd/QdSection";
import { useQd } from "./qd/useQd";
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

/**
 * Оболочка окна (D-155): раскладка, навигация по разделам и баннер. Состояние живёт
 * в контроллерах по доменам (`src/controllers/`), а разделы ядер выбираются в одном месте.
 */
export default function App() {
  const [message, setMessage] = useState<Message | null>(null);
  const [tab, setTab] = useState("connection");
  const { status, setStatus } = useStatus(setMessage);
  const { settings, setSettings, update } = useSettings(setMessage);
  const { sources, setSources, reload: reloadSources } = useSources();
  const { sections, setSections, reload: reloadSections } = useSections(tab);
  const drafts = useDrafts();
  const { job, setJob, busy } = useJob();
  /// Несохранённое в форме клиента: она пишет сразу, но пока поле редактируется —
  /// обновление клиента его бы потеряло.
  const [formDirty, setFormDirty] = useState(false);
  const updates = useUpdates({
    job,
    setJob,
    unsaved: () => formDirty || drafts.unsaved,
    report: setMessage,
    setStatus,
  });
  /// Какое ядро показывают разделы (D-154). Работающее может быть другим — о нём шапка.
  const engine = ENGINES[settings.engine];
  const qd = useQd(settings.engine === "qd" || status.active === "qd");
  const connection = useConnectionActions({
    status,
    setStatus,
    engine: settings.engine,
    setJob,
    report: setMessage,
    afterPower: qd.reload,
  });
  const { clear: clearDrafts } = drafts;
  const afterReset = useCallback(async () => {
    clearDrafts();
    await reloadSources();
  }, [clearDrafts, reloadSources]);
  const system = useSystemActions({
    setStatus,
    setSettings,
    setJob,
    report: setMessage,
    afterReset,
  });

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

  /// Запуск — список хуков, а не ветка в этом файле (D-095). Окно не знает, из чего он
  /// состоит: оно даёт хукам, куда положить добытое, и ждёт, пока они снимут заставку.
  /// Порядок шагов и их сообщения живут в `lifecycle.ts`.
  const { setInfo: setUpdateInfo } = updates;
  useEffect(() => {
    lifecycle.boot({
      settings: setSettings,
      sections: setSections,
      sources: setSources,
      status: setStatus,
      message: setMessage,
      updates: setUpdateInfo,
    });
  }, [setSettings, setSections, setSources, setStatus, setUpdateInfo]);

  const onFocused = useCallback(() => setFocusAdd(false), []);

  /// «Добавить источник» — из шапки и из пустого состояния узлов. Кнопка обещает
  /// «добавить», значит и доводит до поля ввода, а не только до раздела.
  const add = useCallback(() => {
    setTab("sources");
    setFocusAdd(true);
  }, []);

  /// Переключатель ядер — вид, а не питание (D-154). Раздела, которого у ядра нет,
  /// не остаётся на экране.
  const chooseEngine = useCallback(
    async (next: api.Engine) => {
      await update({ engine: next });
      setTab((was) =>
        sections.some((item) => item.id === was) &&
        !ENGINES[next].sections(sections).some((item) => item.id === was)
          ? "connection"
          : was,
      );
    },
    [update, sections],
  );

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
  const shownSections = engine.sections(sections);
  /// Шапка говорит о работающем ядре; ничего не работает — о выбранном.
  const headline = ENGINES[status.active ?? settings.engine].headline({
    status,
    qd: qd.status,
    powering: job === "power",
  });
  const tabs: DockItem[] = [
    ...FIRST.map((item) => ({ ...item, label: t(item.label) })),
    ...shownSections.map((section) => ({
      value: section.id,
      label: section.label,
      icon: ICONS[section.id],
      // Несохранённое в **любом** документе раздела: точка на вкладке отвечает за раздел
      // целиком, иначе правка в «Клиенте» была бы не видна из «Соединения».
      badge: section.docs.some(
        (doc) =>
          drafts.configs[doc.id] !== undefined &&
          drafts.configs[doc.id].text !== drafts.configs[doc.id].saved,
      ),
      badgeLabel: t("unsaved changes"),
    })),
    ...LAST.map((item) => ({ ...item, label: t(item.label) })),
  ];
  /// Dock виден только над разделами: настройки и отладка занимают окно целиком.
  const section = shownSections.find((item) => item.id === tab);
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
  /// Общий редактор разделов конфига. Ядро может добавить в него свой документ — форму
  /// над собственным API (D-154).
  const config = (own?: React.ComponentProps<typeof ConfigEditor>["own"]) =>
    section && (
      <ConfigEditor
        section={section}
        own={own}
        drafts={drafts.configs}
        onDraft={drafts.onDraft}
        onDisk={drafts.onDisk}
        onSections={reloadSections}
        onMessage={setMessage}
        client={{
          settings,
          status,
          busy,
          installing: job === "install",
          updateInfo: updates.info,
          checkingUpdate: updates.checking,
          updateProgress: updates.progress,
          onCheckUpdate: updates.check,
          onClientUpdate: updates.install,
          onUnsavedChange: setFormDirty,
          onChange: update,
          onInstall: connection.install,
          onAutostart: system.autostart,
          onAlwaysAdmin: system.alwaysAdmin,
          onKillSwitch: system.killSwitch,
          onReset: system.reset,
          onStatus: setStatus,
          onLanguageChange: (language) => {
            if (Object.values(drafts.configs).some((draft) => draft.text !== draft.saved)) {
              setMessage(notice(t("Save or discard your edits before changing the language.")));
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
    );

  return (
    // Раскладку (islands / inset), материал и тему решает оформление rootik (D-142).
    <AppShell
      className="um-shell"
      headerShape="none"
      dimWhenInactive
      header={
        <TitleBar
          headline={headline}
          engine={settings.engine}
          onEngine={chooseEngine}
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
      {(updates.open || updates.progress) && (
        <Dialog
          title={t("Client updates")}
          onClose={() => {
            if (!updates.progress) updates.setOpen(false);
          }}
          dismissible={!updates.progress}
          hideClose={updates.progress !== null}
        >
          <ClientUpdate
            info={updates.info}
            checking={updates.checking}
            progress={updates.progress}
            busy={busy}
            onCheck={updates.check}
            onInstall={updates.install}
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
            onAccept={() => system.alwaysAdmin(true)}
            onDismiss={() => update({ adminOffer: false })}
          />
        )}

        <Banner
          message={banner}
          onInstall={connection.install}
          onElevate={system.elevate}
          onRestart={connection.restart}
          onDismiss={message ? () => setMessage(null) : undefined}
        />

        {updates.info?.version && job !== "update" && (
          <Callout
            tone="info"
            title={t("umiray {version} is available", { version: updates.info.version })}
            actions={
              <Button size="sm" onClick={() => updates.setOpen(true)}>
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
              key={`${tab}-${settings.engine}`}
              id="section-panel"
              role="tabpanel"
              aria-labelledby={`tab-${tab}`}
              className="um-section flex min-h-0 flex-1 flex-col gap-3"
            >
              {/* Одна точка выбора ядра (D-154): свои разделы у каждого, общие — ниже. */}
              {settings.engine === "qd" ? (
                <QdSection
                  tab={tab}
                  section={section}
                  qd={qd}
                  status={status}
                  powering={job === "power"}
                  onPower={connection.power}
                  hidden={settings.private}
                  onHidden={() => update({ private: !settings.private })}
                  focus={focusAdd}
                  onFocused={onFocused}
                  onAdd={add}
                  onElevate={system.elevate}
                  onMessage={setMessage}
                  config={config}
                />
              ) : (
                <>
                  {tab === "connection" && (
                    <Connection
                      status={status}
                      mode={connection.mode}
                      busy={busy}
                      powering={job === "power"}
                      onMode={connection.choose}
                      onPower={connection.power}
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
                      drafts={drafts.sources}
                      onDraft={drafts.onSourceDraft}
                      onDisk={drafts.onSourceDisk}
                      onChanged={reloadSources}
                      onMessage={setMessage}
                    />
                  )}
                  {config()}
                </>
              )}
              {tab === "diag" && <Tools onMessage={setMessage} />}
              {tab === "logs" && <Logs engine={settings.engine} hidden={settings.private} />}
            </section>
          </>
        )}
      </div>
    </AppShell>
  );
}
