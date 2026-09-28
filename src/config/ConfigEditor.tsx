import { Cpu, Sparkles } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Card, EmptyState, Select, Spinner } from "rootik";
import * as api from "../api";
import { t } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";
import SaveActions from "../shell/SaveActions";
import Scroll from "../shell/Scroll";
import SectionBar from "../shell/SectionBar";
import ViewSwitch, { type View } from "../shell/ViewSwitch";
import ClientForm, { type ClientProps } from "./ClientForm";
import CoreForm from "./CoreForm";
import { type Drafts, dirty as isDirty } from "./draft";
import Editor from "./Editor";
import GroupsForm from "./GroupsForm";
import PresetPicker from "./PresetPicker";
import RulesForm from "./RulesForm";

type Props = {
  /// Раздел окна вместе с документами внутри (D-070). В «Настройках» их два: конфиг ядра
  /// и настройки клиента (D-068), в остальных разделах — по одному.
  section: api.ConfigSection;
  drafts: Drafts;
  /// Черновики живут у `App`, а не здесь (D-040): переход в другой раздел размонтирует
  /// редактор, и несохранённый YAML иначе пропал бы вместе с ним.
  onDraft: (id: string, text: string) => void;
  /// Что лежит на диске сейчас. Черновик при этом заменяется, **только если его не правили**:
  /// несохранённое принадлежит пользователю, и молча стирать его нельзя.
  onDisk: (id: string, text: string) => void;
  /// Список документов изменился: завели набор или применили другой. Перечитать его может
  /// только `App` — он же его и держит.
  onSections: () => void;
  onMessage: (message: Message | null) => void;
  /// Всё, что нужно форме «Клиента»: статус, настройки и действия над системой (D-089).
  /// Едет одним объектом транзитом — сам редактор про них ничего не знает.
  client: ClientProps;
  own?: { id: string; render: (start: React.ReactNode) => React.ReactNode };
};

/// Значок документа. Список закрытый: документов в «Настройках» ровно два (D-117),
/// и знать их в лицо — работа этой полосы, а не повод заводить реестр.
const ICONS: Record<string, typeof Cpu> = { client: Sparkles, advanced: Cpu, qd: Cpu };

/// Документ настроек клиента. Идентификаторы документов знает и окно — так же, как знает
/// их значки: список закрытый, и второй способ его узнать был бы лишним запросом.
const CLIENT = "client";

/// Документ конфига ядра. У него своя форма (D-086) — не весь файл, а поля, у которых
/// один хозяин; остальное правится кодом.
const ADVANCED = "advanced";

/// Разделы, у которых форма правит **черновик**, а не диск (D-074).
const GROUPS = "groups";
const RULES = "rules";

/// Что сказать после записи.
///
/// Запись доводит до работающего ядра всё, что до него доходит (D-143), — молчать о том,
/// дошло или нет, нельзя. У неприменённого набора новость крупнее: он не работает вовсе,
/// сколько его ни правь (D-071).
function savedText(doc: api.ConfigDoc, presets: boolean, running: boolean): string {
  if (presets && !doc.applied)
    return t("Saved to preset «{name}». Another preset controls the route; choose Use to switch.", {
      name: doc.label,
    });
  if (!doc.core) return t("Saved.");
  return running
    ? t("Saved and applied to the running VPN.")
    : t("Saved. Changes take effect when you connect.");
}

/**
 * Раздел конфига: один или несколько документов и один редактор над ними (D-044, D-070).
 * Полоса разделов осталась наверху окна; какой документ правим — **внутри** полосы
 * редактора, потому что две одинаковые полосы друг под другом читаются как одна.
 * У «Настроек» это две пилюли, у «Маршрутизации» — выпадающий список наборов (D-075),
 * у «Групп» ничего: документ там один и общий.
 *
 * Собранного конфига здесь больше нет — он в «Инструментах» (D-130): кнопка рядом
 * с «Сохранить», которая ничего не сохраняет и гасит всё соседнее, читалась непонятной.
 */
export default function ConfigEditor({
  section,
  drafts,
  onDraft,
  onDisk,
  onSections,
  onMessage,
  client,
  own,
}: Props) {
  const [docId, setDocId] = useState(section.docs[0].id);
  /// Вид — один на раздел, а не на документ (решение пользователя): переключатель стоит
  /// на своём месте в полосе и означает «как смотреть этот раздел», а не «как смотреть
  /// вот этот документ».
  ///
  /// Умолчание — форма, во всех разделах (D-086). Прежнее «раздел открывается
  /// кодом» держалось на том, что формы у него не было вовсе; теперь она есть у обоих
  /// его документов, и код стал вторым способом смотреть на то же самое.
  const [view, setView] = useState<View>("visual");
  const [rendering, setRendering] = useState(false);

  const doc = section.docs.find((item) => item.id === docId) ?? section.docs[0];
  const draft = drafts[doc.id];
  const dirty = isDirty(draft);
  /// Правит ли форма этого документа черновик, а не диск (D-074). У «Групп»
  /// и «Маршрутизации» — черновик, у «Ядра» и «Клиента» — диск (D-052), поэтому кнопок
  /// записи над ними нет: они писали бы поверх того, что форма уже записала.
  const drafted = section.id === GROUPS || section.id === RULES;

  /// Содержимое тянем по одному документу — и **каждый раз, когда его открывают**, а не
  /// один раз за жизнь окна.
  ///
  /// В эти же файлы пишет не только редактор: переключатель режима правит `advanced.yaml`,
  /// форма — `client.yaml`, смена направления перекладывает `groups.yaml` и `rules.yaml`
  /// (D-056). Кэш без сверки с диском показывал бы текст до этих записей, а сохранение
  /// поверх него откатывало бы их обратно — то есть окно тихо отменяло бы собственный
  /// переключатель.
  const reload = useCallback(() => {
    if (doc.id === own?.id) return;
    api.configRead(doc.id).then(
      (text) => onDisk(doc.id, text),
      (e) => onMessage(failure(e)),
    );
  }, [doc.id, own?.id, onDisk, onMessage]);

  useEffect(reload, [reload]);

  // Смена раздела возвращает к первому документу: в чужом разделе выбранный указывал бы
  // в никуда. Подтверждение сброса относится к открытому документу.
  // biome-ignore lint/correctness/useExhaustiveDependencies: сброс по смене раздела
  useEffect(() => {
    setDocId(section.docs[0].id);
    setView("visual");
  }, [section.id]);

  const save = async () => {
    if (draft === undefined) return;
    onMessage(null);
    try {
      const status = await api.configWrite(doc.id, draft.text);
      client.onStatus(status);
      onDisk(doc.id, draft.text);
      onMessage(notice(savedText(doc, section.presets, status.running)));
    } catch (e) {
      onMessage(failure(e));
    }
  };

  /// Применить набор: с этого момента маршрут решают его документы. Ядро читает их
  /// на старте (D-010), поэтому работающее перезапускается — об этом и говорим.
  const apply = async () => {
    const preset = doc.id.split("/")[1];
    onMessage(null);
    try {
      await api.presetsSelect(preset);
      onSections();
      // Про перезапуск — только если было что перезапускать: у выключенного VPN набор
      // просто вступит в силу при подключении.
      onMessage(
        notice(
          client.status.running
            ? t("Preset «{name}» is now in use. The running core was restarted to apply it.", {
                name: doc.label,
              })
            : t("Preset «{name}» is now in use and takes effect when you connect.", {
                name: doc.label,
              }),
        ),
      );
    } catch (e) {
      onMessage(failure(e));
    }
  };

  /// Завести набор — копию собранного клиентом — и сразу открыть его.
  const create = async () => {
    onMessage(null);
    try {
      const created = await api.presetsCreate();
      onSections();
      setDocId(`${section.id}/${created.id}`);
    } catch (e) {
      onMessage(failure(e));
    }
  };

  /// Переименовать набор. Имя приводит к уникальному бэкенд: два набора с одной подписью
  /// в списке неразличимы.
  const rename = async (name: string) => {
    onMessage(null);
    try {
      await api.presetsRename(doc.id.split("/")[1], name);
      onSections();
    } catch (e) {
      onMessage(failure(e));
    }
  };

  /// Удалить набор. Применённый и последний оставшийся бэкенд не отдаёт — и говорит,
  /// почему: пустое нажатие объясняло бы это молчанием.
  const remove = async () => {
    onMessage(null);
    try {
      await api.presetsDelete(doc.id.split("/")[1]);
      // Открываем **соседний**, а не первый: список документов обновится только ближайшим
      // опросом, и до него `section.docs[0]` вполне может быть тем самым удалённым —
      // раздел попытался бы его прочитать и показал бы ошибку на ровном месте.
      const rest = section.docs.filter((item) => item.id !== doc.id);
      if (rest.length > 0) setDocId(rest[0].id);
      onSections();
    } catch (e) {
      onMessage(failure(e));
    }
  };

  const reset = async () => {
    onMessage(null);
    try {
      const text = await api.configReset(doc.id);
      onDisk(doc.id, text);
      // Сброс заменяет и несохранённое тоже — ровно за этим его и нажимают, и второе
      // подтверждение стоит именно поэтому.
      onDraft(doc.id, text);
    } catch (e) {
      onMessage(failure(e));
    }
  };

  /// Какой документ правим — в «Настройках» (два документа). У «Маршрутизации» это
  /// выбор набора, у «Групп» документ один, и выбирать нечего.
  const docSwitch =
    section.docs.length > 1 && !section.presets ? (
      <Select
        className="w-[220px]"
        aria-label={t("Settings document")}
        value={doc.id}
        onChange={setDocId}
        options={section.docs.map((item) => {
          const Icon = ICONS[item.id] ?? Sparkles;
          return {
            value: item.id,
            label: `${item.label}${isDirty(drafts[item.id]) ? " •" : ""}`,
            icon: <Icon />,
          };
        })}
      />
    ) : null;

  // Слева — что и как смотрим, справа — что сделать (`SectionBar`). Вид первым: он стоит
  // на одном месте в каждом разделе.
  const start = (
    <>
      {doc.id !== own?.id && <ViewSwitch value={view} onChange={setView} />}
      {section.presets ? (
        // Наборы — выпадающим списком, действия над ними — в меню рядом (D-075).
        <PresetPicker
          docs={section.docs}
          value={doc.id}
          onChange={setDocId}
          dirty={(id) => isDirty(drafts[id])}
          onApply={apply}
          onCreate={create}
          onRename={rename}
          onDelete={remove}
        />
      ) : (
        docSwitch
      )}
    </>
  );

  const actions = draft !== undefined && (
    <SaveActions
      dirty={dirty}
      busy={rendering}
      onUndo={() => onDraft(doc.id, draft.saved)}
      onSave={save}
      onReset={doc.id !== GROUPS ? reset : undefined}
    />
  );

  // «Настройки» формой — страница с оглавлением; кнопки записи у «Ядра» свои, у «Клиента»
  // их нет вовсе: каждая галка пишется сразу (D-117).
  if (own && doc.id === own.id) {
    return <>{own.render(start)}</>;
  }

  if (view === "visual" && (doc.id === CLIENT || doc.id === ADVANCED)) {
    return doc.id === CLIENT ? (
      // Форма пишет в тот же документ, что открыт в коде, — после записи его перечитать (D-052).
      <ClientForm
        {...client}
        onMessage={onMessage}
        onSaved={reload}
        start={start}
        hint={doc.hint}
      />
    ) : (
      <CoreForm
        onMessage={onMessage}
        onSaved={reload}
        start={start}
        hint={doc.hint}
        onUnsavedChange={client.onUnsavedChange}
      />
    );
  }

  if (view === "visual") {
    return (
      <>
        <SectionBar start={start} end={drafted && actions} hint={doc.hint} />
        <Scroll>
          {draft === undefined ? (
            <Spinner label={t("Loading")} />
          ) : section.id === GROUPS ? (
            <GroupsForm
              text={draft.text}
              onDraft={(text) => onDraft(doc.id, text)}
              onMessage={onMessage}
            />
          ) : section.id === RULES ? (
            <RulesForm
              text={draft.text}
              onDraft={(text) => onDraft(doc.id, text)}
              onMessage={onMessage}
              onPending={setRendering}
            />
          ) : (
            <Card>
              <EmptyState
                title={t("«{name}» as a form", { name: doc.label })}
                hint={t("This document can only be edited as text — select Code.")}
              />
            </Card>
          )}
        </Scroll>
      </>
    );
  }

  return (
    <>
      <SectionBar start={start} end={actions} hint={doc.hint} />
      {/* Редактор — во всю оставшуюся высоту и со своей прокруткой. */}
      <Card className="min-h-0 flex-1" padding="sm">
        {draft === undefined ? (
          <Spinner label={t("Loading")} />
        ) : (
          // Ключ по документу: у разных файлов разная история отмены, и мешать их нельзя.
          <div className="h-full min-h-0">
            <Editor key={doc.id} value={draft.text} onChange={(text) => onDraft(doc.id, text)} />
          </div>
        )}
      </Card>
    </>
  );
}
