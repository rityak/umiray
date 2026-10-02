import { Layers, Network, Route, ScrollText, SlidersHorizontal } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import {
  AppShell,
  Button,
  Callout,
  ConfirmHost,
  Dialog,
  Dock,
  type DockItem,
  Toaster,
} from "rootik";
import * as api from "./api";
import TitleBar from "./chrome/TitleBar";
import ClientUpdate from "./config/ClientUpdate";
import ConfigEditor from "./config/ConfigEditor";
import Connection from "./connection/Connection";
import { useAdding } from "./controllers/useAdding";
import { useConnectionActions } from "./controllers/useConnectionActions";
import { useDrafts } from "./controllers/useDrafts";
import { useJob } from "./controllers/useJob";
import { useSections } from "./controllers/useSections";
import { useSettings } from "./controllers/useSettings";
import { useSources } from "./controllers/useSources";
import { useStatus } from "./controllers/useStatus";
import { useSystemActions } from "./controllers/useSystemActions";
import { useUpdates } from "./controllers/useUpdates";
import { ENGINES, headline } from "./engines";
import { saveLanguagePreference, t, tk } from "./i18n";
import * as lifecycle from "./lifecycle";
import Logs from "./logs/Logs";
import QdSection from "./qd/QdSection";
import { useQd } from "./qd/useQd";
import Settings from "./settings/Settings";
import Setup from "./setup/Setup";
import AdminOffer from "./shell/AdminOffer";
import Banner, { failure, type Message, notice } from "./shell/Banner";
import Debug from "./shell/Debug";
import LinkDialog from "./sources/LinkDialog";
import NodeDialog from "./sources/NodeDialog";
import WarpDialog from "./sources/WarpDialog";

/// Постоянные разделы. Между ними встают разделы конфига: «Группы», «Маршрутизация»,
/// «Настройки» (D-044). Документов внутри может быть больше одного (D-070), но полосе
/// разделов это безразлично. Подписи знает бэкенд, и держать вторую копию списка здесь
/// значило бы разъехаться при первом же переименовании.

/// Значок на раздел. Ключи файлов конфига приходят из `files::list()`, поэтому карта
/// по идентификатору, а не по порядку: переименуют раздел — значок останется на месте.
const ICONS: Record<string, React.ReactNode> = {
  connection: <Network />,
  groups: <Layers />,
  rules: <Route />,
  advanced: <SlidersHorizontal />,
  logs: <ScrollText />,
};

/// Источники — вид внутри «Соединения», а не раздел (D-160).
const FIRST: DockItem[] = [
  { value: "connection", label: tk("Connection"), icon: ICONS.connection },
];
/// Логи — после конфига: сначала настраивают, потом читают вывод ядра. Раздела
/// «Инструменты» нет: замер стоит там, где нужен его ответ (D-115, D-167).
const LAST: DockItem[] = [{ value: "logs", label: tk("Logs"), icon: ICONS.logs }];

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
  /// qd виден, только пока он скачан (D-161): пропал файл — вид возвращается к mihomo.
  const viewed: api.Engine = status.qdPresent ? settings.engine : "mihomo";
  const engine = ENGINES[viewed];
  const qd = useQd(viewed === "qd" || status.active === "qd");
  const connection = useConnectionActions({
    status,
    setStatus,
    setSettings,
    engine: viewed,
    setJob,
    report: setMessage,
    afterPower: qd.reload,
  });
  const { clear: clearDrafts } = drafts;
  const afterReset = useCallback(async () => {
    clearDrafts();
    await reloadSources();
  }, [clearDrafts, reloadSources]);
  /// Добавили источник — откуда угодно (D-160): перечитать источники, статус (qd мог
  /// появиться) и состояние qd.
  const { reload: reloadQd } = qd;
  const afterAdd = useCallback(async () => {
    await reloadSources();
    setStatus(await api.coreStatus());
    reloadQd();
  }, [reloadSources, setStatus, reloadQd]);
  const adding = useAdding({ report: setMessage, onAdded: afterAdd });
  /// Мастер первого запуска (D-162): открывает шаг запуска `setup` или кнопка в настройках.
  const [setupOpen, setSetupOpen] = useState(false);
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
      setup: () => setSetupOpen(true),
    });
  }, [setSettings, setSections, setSources, setStatus, setUpdateInfo]);

  /// Мастер пройден (D-162): запомнить это и настроенное ядро.
  const setupDone = useCallback(
    async (next: api.Engine) => {
      await update({ setup: true, engine: next });
      setSetupOpen(false);
    },
    [update],
  );

  /// Переключатель ядер — вид, а не питание (D-154). Раздела, которого у ядра нет,
  /// не остаётся на экране: вкладка меняется в том же кадре, что и вид, — иначе между
  /// ними мелькал пустой раздел.
  const chooseEngine = useCallback(
    (next: api.Engine) => {
      const swap = () => {
        setTab((was) =>
          sections.some((item) => item.id === was) &&
          !ENGINES[next].sections(sections).some((item) => item.id === was)
            ? "connection"
            : was,
        );
        void update({ engine: next });
      };
      // Окно сменяется кроссфейдом целиком, а не гасит раздел в ноль (STYLEGUIDE).
      if (document.startViewTransition) document.startViewTransition(() => flushSync(swap));
      else swap();
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
          text: status.trouble.text,
          details: [],
          // Кнопка — только там, где перезапуск лечит: подписке, которая не обновилась, он
          // не поможет.
          kind: status.trouble.restart ? "restartNeeded" : undefined,
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
  const header = headline({ status, qd: qd.status, powering: job === "power" });
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
          onQd: connection.qdSwitch,
          onRouting: connection.routing,
          onSetup: () => setSetupOpen(true),
          onExport: system.exportSettings,
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
          headline={header}
          engine={viewed}
          engines={status.qdPresent ? ["mihomo", "qd"] : ["mihomo"]}
          onEngine={chooseEngine}
          onAdd={adding.pick}
          adds={engine.adds}
          onSetup={() => setSetupOpen(true)}
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
      {/* Тосты живут в верхнем слое: видны и поверх мастера (D-161). */}
      <Toaster position="bottom-right" />
      <ConfirmHost />
      {/* Окна добавления — одни на все кнопки «+» (D-160). */}
      {adding.open === "link" && (
        <LinkDialog onSubmit={adding.submit} onClose={adding.close} qdOnly={viewed === "qd"} />
      )}
      {adding.open === "warp" && <WarpDialog onDone={adding.done} onClose={adding.close} />}
      {adding.open === "node" && <NodeDialog onDone={adding.done} onClose={adding.close} />}
      {setupOpen && (
        <Setup
          again={settings.setup}
          status={status}
          sources={sources}
          mode={connection.mode}
          onLink={adding.link}
          onFile={adding.file}
          onElevate={system.elevate}
          onMode={async (mode) => {
            const refused = await connection.choose(mode);
            if (refused) throw refused;
          }}
          onStatus={setStatus}
          onDone={setupDone}
          // Уже работает — не гасим: мастер, открытый снова, правит живое соединение.
          onConnect={async () => {
            if (status.active === null) await connection.power();
          }}
          onClose={() => {
            setSetupOpen(false);
            update({ setup: true });
          }}
        />
      )}
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
              это уже не предложение. И не поверх мастера: два вопроса разом —
              ни на один не ответят. */}
        {status.elevated && !status.alwaysAdmin && settings.adminOffer && !setupOpen && (
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
              key={tab}
              id="section-panel"
              role="tabpanel"
              aria-labelledby={`tab-${tab}`}
              className="um-section flex min-h-0 flex-1 flex-col gap-3"
            >
              {/* Одна точка выбора ядра (D-154): свои разделы у каждого, общие — ниже. */}
              {viewed === "qd" ? (
                <QdSection
                  tab={tab}
                  section={section}
                  qd={qd}
                  status={status}
                  powering={job === "power"}
                  onPower={connection.power}
                  hidden={settings.private}
                  onHidden={() => update({ private: !settings.private })}
                  onAdd={adding.pick}
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
                      onAdd={adding.pick}
                      onMessage={setMessage}
                      schedule={settings.refresh}
                      onSchedule={(refresh) => update({ refresh })}
                      drafts={drafts.sources}
                      onDraft={drafts.onSourceDraft}
                      onDisk={drafts.onSourceDisk}
                      onSourcesChanged={reloadSources}
                    />
                  )}
                  {config()}
                </>
              )}
              {tab === "logs" && <Logs key={viewed} engine={viewed} hidden={settings.private} />}
            </section>
          </>
        )}
      </div>
    </AppShell>
  );
}
