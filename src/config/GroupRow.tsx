import { ChevronDown, ChevronUp, Trash2 } from "lucide-react";
import {
  Badge,
  Callout,
  Card,
  ChoiceCards,
  ConfirmButton,
  Field,
  IconButton,
  Input,
  NumberInput,
  SectionLabel,
  Select,
  Text,
} from "rootik";
import type * as api from "../api";
import IconPicker from "../connection/IconPicker";
import { t, tk } from "../i18n";
import Flag from "../shell/Flag";
import { type Choices, live, nodesOf, preview, type Selection, whole, write } from "./groups";
import { GROUP_KIND, UNKNOWN_KIND } from "./kinds";
import NodeTree from "./NodeTree";

/// Адрес проверки по умолчанию. `generate_204` отвечает пустым ответом и не кэшируется —
/// им и меряют доступность; гугловский выбран решением пользователя.
export const HEALTH = "http://www.google.com/generate_204";
const INTERVAL = 300;
const TOLERANCE = 150;

/// Четыре типа, которые понимает форма. Заголовок — термин ядра, объяснение — по-русски
/// (STYLEGUIDE): переводить `url-test` значило бы отнять у пользователя слово, которым
/// это ищется в документации.
const KINDS: { id: string; about: string }[] = [
  { id: "select", about: tk("you choose manually") },
  { id: "url-test", about: tk("fastest response") },
  { id: "fallback", about: tk("first reachable in order") },
  { id: "load-balance", about: tk("distributes across all nodes") },
];

const STRATEGIES: { id: string; about: string }[] = [
  { id: "consistent-hashing", about: tk("the same address always uses the same server") },
  { id: "round-robin", about: tk("rotate servers for each connection") },
  { id: "sticky-sessions", about: tk("keep each client on one server") },
];

/// Имена групп, которые клиент собирает сам (D-053, D-113, D-072). Своя группа с таким
/// именем останавливает сборку (D-135) — говорим об этом у поля, а не при подключении.
const RESERVED = ["AUTO", "umiray", "umiray-udp", "probe"];

/// Что не так с именем группы — то, из-за чего ядро не поднимется. Пусто — всё в порядке.
function nameError(name: string, taken: string[]): string | undefined {
  const trimmed = name.trim();
  if (trimmed === "") return t("Name the group — the core won't accept one without a name.");
  if (RESERVED.includes(trimmed) || trimmed.startsWith("umiray-"))
    return t("The client uses this name for its own group. Pick another one.");
  if (taken.includes(trimmed))
    return t("Another group already has this name. The core won't accept two.");
  // Ядро режет строку правила по запятой: правило в такую группу не направить (B-043).
  if (trimmed.includes(","))
    return t("A rule can't lead to a name with a comma — the core cuts rules at it.");
  return undefined;
}

type Props = {
  group: api.Group;
  selection: Selection;
  /// Names of the other groups: two groups with one name break the whole config.
  taken: string[];
  choices: Choices;
  open: boolean;
  onToggle: () => void;
  /// Значок — общий с «Соединением» (`Settings.group_icons`, D-172).
  icon: string | null;
  onIcon: (id: string | null) => void;
  onChange: (group: api.Group, selection: Selection) => void;
  onRemove: () => void;
};

/// Из чего собрана группа, словами: «Demo VPN» целиком (8) · «Мои ссылки»: 2 из 11.
function pickedParts(choices: Choices, picked: Set<string>): string[] {
  return choices.sources.flatMap((source) => {
    const nodes = nodesOf(choices, source.id);
    const mine = nodes.filter((node) => picked.has(node.name)).length;
    if (mine === 0) return [];
    return [
      whole(choices, source.id, picked)
        ? t("all of «{name}»", { name: source.name })
        : t("«{name}»: {n} of {total}", { name: source.name, n: mine, total: nodes.length }),
    ];
  });
}

/**
 * Одна группа: строка-заголовок и всё её содержимое под ней.
 *
 * Раскрыта всегда одна — правят группы по одной, а четыре раскрытых сразу превращают
 * список в простыню, в которой не видно, что с чем рядом.
 */
export default function GroupRow({
  group,
  selection,
  taken,
  choices,
  open,
  onToggle,
  icon,
  onIcon,
  onChange,
  onRemove,
}: Props) {
  const set = (patch: Partial<api.Group>) => onChange({ ...group, ...patch }, selection);
  const pick = (next: Selection) => onChange(write(group, next, choices), next);

  const picked = new Set(selection.picked);
  const parts = pickedParts(choices, picked);
  const rows = preview(group, selection, choices);
  const inside = rows.filter((row) => !row.out).length;
  const checks = group.kind !== "select" && group.kind !== "";
  const balance = group.kind === "load-balance";
  const tolerance = group.kind === "url-test";
  const isLive = live(choices, picked);
  const strategy = STRATEGIES.find((item) => item.id === group.strategy);
  /// Что в этой группе форма не понимает: незнакомые поля и фильтр, написанный руками.
  const coded = [...group.extra, ...(selection.understood ? [] : [`filter: ${group.filter}`])];

  const kind = (id: string) =>
    set({
      kind: id,
      // Проверка — часть типа: url-test без адреса и интервала не проверяет ничего,
      // и оставлять поля пустыми значило бы отдать решение ядру молча.
      url: id === "select" ? group.url : (group.url ?? HEALTH),
      interval: id === "select" ? group.interval : (group.interval ?? INTERVAL),
      tolerance: id === "url-test" ? (group.tolerance ?? TOLERANCE) : group.tolerance,
      strategy: id === "load-balance" ? (group.strategy ?? STRATEGIES[0].id) : group.strategy,
    });

  const look = GROUP_KIND[group.kind] ?? UNKNOWN_KIND;
  const badName = nameError(group.name, taken);
  const country = (name: string) =>
    choices.nodes.find((node) => node.name === name)?.country ?? null;
  // В заголовке — страны состава: по ним группу узнают быстрее, чем по имени.
  const flags = [
    ...new Set(
      rows
        .filter((row) => !row.out)
        .map((row) => country(row.name))
        .filter(Boolean),
    ),
  ].slice(0, 6) as string[];

  return (
    // Не `collapsible`: его заголовок — одна кнопка, и выбор значка в нём был бы кнопкой
    // в кнопке. Раскрывает шеврон, как у карточки группы в «Соединении».
    <Card
      headingLevel={3}
      media={
        <IconPicker
          value={icon}
          fallback={look.Icon}
          label={t("Icon of {name}", { name: group.name || t("unnamed") })}
          onChange={onIcon}
        />
      }
      title={group.name || t("unnamed")}
      description={`${t(look.word)} · ${
        parts.length > 0
          ? parts.join(" · ")
          : selection.others.length > 0
            ? t("by name: {n}", { n: selection.others.length })
            : t("nothing selected")
      }`}
      actions={
        <span className="flex items-center gap-2">
          {/* Свёрнутая группа тоже говорит, что с ней ядро не поднимется. */}
          {badName && (
            <Badge tone="danger" size="sm">
              {t("check the name")}
            </Badge>
          )}
          <span className="flex items-center gap-1" aria-hidden="true">
            {flags.map((code) => (
              <Flag key={code} country={code} />
            ))}
          </span>
          {/* Имена, записанные руками (другие группы, DIRECT), — тоже состав группы. */}
          <Badge tone={inside + selection.others.length === 0 ? "warn" : "neutral"}>
            {t("{n} members", { n: inside + selection.others.length })}
          </Badge>
          <IconButton
            size="sm"
            variant="ghost"
            icon={open ? <ChevronUp /> : <ChevronDown />}
            label={open ? t("Fold") : t("Edit group")}
            aria-expanded={open}
            onClick={onToggle}
          />
        </span>
      }
    >
      {open && (
        // Between sections twice the space inside one: the form reads as blocks, not a list.
        <div className="um-swap flex flex-col gap-6">
          <Field label={t("Group name")} error={badName}>
            <Input value={group.name} onChange={(event) => set({ name: event.target.value })} />
          </Field>

          <section className="flex flex-col gap-2">
            <SectionLabel>{t("Node selection")}</SectionLabel>
            <ChoiceCards
              aria-label={t("How the group selects a node")}
              minWidth={150}
              value={group.kind}
              onChange={kind}
              options={KINDS.map((item) => ({
                value: item.id,
                label: item.id,
                description: t(item.about),
                icon: (() => {
                  const { Icon } = GROUP_KIND[item.id] ?? UNKNOWN_KIND;
                  return <Icon />;
                })(),
              }))}
            />
          </section>

          {checks && (
            <section className="flex flex-col gap-2">
              <SectionLabel>{t("Health check")}</SectionLabel>
              <div className="grid grid-cols-[minmax(0,1fr)_120px_180px] items-start gap-2 max-[640px]:grid-cols-1">
                <Field label={t("Check URL")}>
                  <Input
                    mono
                    value={group.url ?? ""}
                    placeholder={HEALTH}
                    onChange={(event) => set({ url: event.target.value || null })}
                  />
                </Field>
                <Field label={t("Interval, s")}>
                  <NumberInput
                    allowEmpty
                    min={1}
                    value={group.interval}
                    placeholder={String(INTERVAL)}
                    onChange={(interval) => set({ interval })}
                  />
                </Field>
                {tolerance && (
                  <Field
                    label={t("Tolerance, ms")}
                    hint={t("Switch only when the difference exceeds the tolerance")}
                  >
                    <NumberInput
                      allowEmpty
                      min={0}
                      value={group.tolerance}
                      placeholder={String(TOLERANCE)}
                      onChange={(tolerance) => set({ tolerance })}
                    />
                  </Field>
                )}
                {balance && (
                  <Field label={t("Strategy")}>
                    <Select
                      value={group.strategy ?? STRATEGIES[0].id}
                      onChange={(value) => set({ strategy: value })}
                      options={STRATEGIES.map((item) => ({
                        value: item.id,
                        label: item.id,
                        hint: t(item.about),
                      }))}
                    />
                  </Field>
                )}
              </div>
              {balance && (
                <Text tone="muted" size="xs" className="block">
                  {t("Strategy: {strategy}", {
                    strategy: t(strategy?.about ?? STRATEGIES[0].about),
                  })}
                </Text>
              )}
            </section>
          )}

          <section className="flex flex-col gap-2">
            <SectionLabel>{t("Members")}</SectionLabel>
            <NodeTree
              choices={choices}
              picked={selection.picked}
              disabled={!selection.understood}
              onPicked={(next) => pick({ ...selection, picked: next })}
            />
            {selection.others.length > 0 && (
              <Text tone="muted" size="xs" className="block">
                {t(
                  "Also in the group: {names} — other groups, DIRECT or names typed by hand. Edit them in Code.",
                  { names: selection.others.join(" · ") },
                )}
              </Text>
            )}
            {selection.understood && isLive && parts.length > 0 && (
              <Field label={t("Keep only nodes whose names contain this text")}>
                <Input
                  value={selection.substring}
                  placeholder={t("e.g. Poland — leave empty to include all")}
                  onChange={(event) => pick({ ...selection, substring: event.target.value })}
                />
              </Field>
            )}
          </section>

          <Card
            variant="sunken"
            padding="sm"
            title={t("The group includes {n} of {total}", { n: inside, total: rows.length })}
            actions={
              <Badge tone={isLive ? "success" : "warn"} dot>
                {isLive ? t("live list") : t("fixed list")}
              </Badge>
            }
          >
            <div className="flex flex-col gap-2">
              <div className="flex flex-wrap gap-1">
                {rows.map((row) => (
                  // Флаг — в слоте `media`: квадратный `icon` сплющил бы его. Входящий — обычный
                  // бейдж, срезанный — контур и зачёркнут.
                  <Badge
                    key={row.name}
                    size="sm"
                    variant={row.out ? "outline" : "soft"}
                    className={row.out ? "line-through" : undefined}
                    media={country(row.name) && <Flag country={country(row.name)} />}
                  >
                    {row.name}
                  </Badge>
                ))}
              </div>
              {inside === 0 && (
                <Callout tone="warn">
                  {t("No nodes matched — the core will reject this group.")}
                </Callout>
              )}
              {!isLive && parts.length > 0 && (
                <Text tone="muted" size="xs" className="block">
                  {t(
                    "Part of a source is saved as exact names, so new subscription nodes won't join. Check the whole source to keep the list live.",
                  )}
                </Text>
              )}
            </div>
          </Card>

          {coded.length > 0 && (
            <Callout tone="warn">
              {t(
                "The form doesn't know these fields and keeps them as is: {fields}. Edit them in Code.",
                {
                  fields: coded.join(", "),
                },
              )}
            </Callout>
          )}

          <div className="flex justify-end">
            <ConfirmButton
              variant="danger"
              icon={<Trash2 />}
              confirmLabel={t("Delete for sure?")}
              onConfirm={onRemove}
            >
              {t("Delete group")}
            </ConfirmButton>
          </div>
        </div>
      )}
    </Card>
  );
}
