import { CornerDownRight, Plus } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Button, Card, ChipGroup, Divider, EmptyState, SegmentedControl, Text } from "rootik";
import * as api from "../api";
import { useCached } from "../hooks/useCached";
import { t } from "../i18n";
import { failure, type Message } from "../shell/Banner";
import GeoBases from "./GeoBases";
import { lostLook, targetLook } from "./kinds";
import ReadySets from "./ReadySets";
import RuleRow from "./RuleRow";
import RuleSets from "./RuleSets";
import TargetPicker from "./TargetPicker";

/// Две страницы одного документа (D-158): маршрут над своими правилами и сами правила.
type Page = "route" | "custom";

type Props = {
  /// Черновик документа — тот же, что открыт в коде (D-074).
  text: string;
  onDraft: (text: string) => void;
  onMessage: (message: Message | null) => void;
  onPending: (pending: boolean) => void;
  /// The core is running — rule sets need it to update the core's geo databases.
  running: boolean;
};

/// Виды правил в списке. Не весь список ядра — распространённое; чего здесь нет, форма
/// всё равно покажет: незнакомый вид дописывается к списку из самого документа.
const KINDS = [
  "DOMAIN-SUFFIX",
  "DOMAIN",
  "DOMAIN-KEYWORD",
  "DOMAIN-REGEX",
  "IP-CIDR",
  "GEOIP",
  "PROCESS-NAME",
  "PROCESS-NAME-REGEX",
  "RULE-SET",
];

/// Назначения, которые есть всегда: псевдоним, автовыбор клиента и два конца — мимо
/// и в никуда.
const TARGETS = ["umiray", "AUTO", "DIRECT", "REJECT"];

/// Exits the core always has: a rule may name them, the lists just don't offer them.
const EXITS = ["REJECT-DROP", "PASS", "COMPATIBLE", "GLOBAL"];

const EMPTY: api.Routing = { rules: [], fallback: "umiray", ruleSets: [], ready: [] };

let kept: { text: string; routing: api.Routing } | null = null;

/// Список без повторов, порядок первого появления.
function unique(items: string[]): string[] {
  return items.filter((item, at) => items.indexOf(item) === at);
}

/**
 * Раздел «Маршрутизация» формой — две страницы одного документа (D-158). «Маршрут»:
 * rule sets, готовые наборы и build-in (MATCH). «Свои правила»: строки над ними. Всё это
 * действует, пока маршрутизация включена (D-166). Код у страниц общий.
 *
 * Порядок здесь — это и есть смысл: побеждает первое совпавшее сверху. Поэтому фишки
 * назначений сверху **фильтруют**, а не перекладывают: номера остаются общими для всего
 * списка, и по ним видно, что выше и что ниже спрятанного.
 */
export default function RulesForm({ text, onDraft, onMessage, onPending, running }: Props) {
  const [routing, setRouting] = useState<api.Routing>(() =>
    kept?.text === text ? kept.routing : EMPTY,
  );
  kept = { text, routing };
  /// `null` — not read yet or unreadable: then no target is called lost.
  const [groupNames, setGroups] = useCached<string[] | null>("rules.groups", null);
  const groups = groupNames ?? [];
  /// Узлы как второй список «куда» (D-082). Спрашиваем у бэкенда, а не считаем из групп:
  /// состав узлов знает каталог источников, а не документ.
  const [nodeNames, setNodes] = useCached<string[] | null>("rules.nodes", null);
  const nodes = nodeNames ?? [];
  /// Downloaded rule sets by name — what a hand-written `RULE-SET` rule can point at (D-157).
  const [lists, setLists] = useCached<string[]>("rules.lists", []);
  const [page, setPage] = useCached<Page>("rules.page", "route");
  const [filter, setFilter] = useState("all");
  const [refused, setRefused] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<number | null>(null);
  const revision = useRef(0);
  const base = useRef<string | null>(null);
  const ours = useRef<string | null>(null);
  /// Куда вернуть фокус после перестановки: стрелки — не украшение, порядок меняется
  /// и с клавиатуры.
  const focus = useRef<string | null>(null);

  useEffect(() => {
    // Группы — общий документ (D-075), и они же половина списка «куда отправить».
    // Читаем документ, а не берём из окна: правит его соседний раздел, и знать про него
    // здесь больше нечего.
    api
      .configRead("groups")
      .then(api.groupsParse)
      .then(
        (list) => setGroups(list.map((group) => group.name)),
        () => setGroups(null),
      );
  }, []);

  useEffect(() => {
    // Узел, которого ядро не поднимет, целью быть не может (D-063): группа под него
    // получилась бы пустой, а такую ядро не принимает.
    api.nodesList().then(
      (list) => setNodes(list.filter((node) => node.supported).map((node) => node.name)),
      () => setNodes(null),
    );
    api.listsList().then(
      (list) => setLists(list.map((item) => item.id)),
      () => setLists([]),
    );
  }, []);

  useEffect(() => {
    // Сверяемся только со своим выводом. Сравнение с `base` было бы ошибкой: «Откатить»
    // возвращает документ ровно к нему, и форма осталась бы показывать правку,
    // которой в черновике уже нет.
    if (text === ours.current) return;
    const current = ++revision.current;
    onPending(false);
    base.current = text;
    let alive = true;
    api.rulesParse(text).then(
      (parsed) => {
        if (!alive || current !== revision.current) return;
        setRefused(null);
        setRouting(parsed);
      },
      (e) => alive && current === revision.current && setRefused(api.asAppError(e).message),
    );
    return () => {
      alive = false;
    };
  }, [text, onPending]);

  useEffect(() => {
    const key = focus.current;
    if (key === null) return;
    focus.current = null;
    const button = document.querySelector<HTMLButtonElement>(`[data-focus="${key}"]`);
    if (button && !button.disabled) {
      button.focus();
      return;
    }
  });

  const commit = useCallback(
    async (next: api.Routing) => {
      const current = ++revision.current;
      onPending(true);
      setRouting(next);
      try {
        const rendered = await api.rulesRender(base.current ?? "", next);
        if (current !== revision.current) return;
        ours.current = rendered;
        onDraft(rendered);
      } catch (e) {
        if (current === revision.current) onMessage(failure(e));
      } finally {
        if (current === revision.current) onPending(false);
      }
    },
    [onDraft, onMessage, onPending],
  );

  if (refused !== null) {
    return (
      <Card>
        <EmptyState
          tone="warn"
          title={t("This document can only be edited as code")}
          hint={`${refused} ${t("Open Code to edit the document without losing anything.")}`}
        />
      </Card>
    );
  }

  // Имена узлов из списка групп убраны: у них теперь свой список за кнопкой, и показывать
  // «Poland 1» среди групп значило бы предлагать одно и то же дважды.
  const targets = unique([
    ...TARGETS,
    ...groups,
    ...routing.rules.map((rule) => rule.target),
    ...routing.ruleSets.map((set) => set.target),
    ...routing.ready.flatMap((set) => (set.target ? [set.target] : [])),
  ]).filter((target) => !nodes.includes(target));
  const kinds = unique([...KINDS, ...routing.rules.map((rule) => rule.kind)]);
  const seen = unique(routing.rules.map((rule) => rule.target));
  const shown = routing.rules
    .map((rule, index) => ({ rule, index }))
    .filter((row) => filter === "all" || row.rule.target === filter);

  const move = (index: number, delta: -1 | 1) => {
    const next = [...routing.rules];
    const to = index + delta;
    if (to < 0 || to >= next.length) return;
    const [moved] = next.splice(index, 1);
    next.splice(to, 0, moved);
    focus.current = `${to + 1}:menu`;
    setExpanded((was) => (was === index ? to : was === to ? index : was));
    commit({ ...routing, rules: next });
  };

  /// A target nothing answers to: a renamed group, a node the subscription dropped. The build
  /// sends it to `umiray` (D-156) — say so at the rule, not "group · through VPN".
  const lost = (target: string) =>
    groupNames !== null &&
    nodeNames !== null &&
    ![...TARGETS, ...EXITS, ...groups, ...nodes].includes(target);
  const fallback = lost(routing.fallback) ? lostLook() : targetLook(routing.fallback, nodes);

  return (
    <div className="flex flex-col gap-3">
      <SegmentedControl
        aria-label={t("Routing page")}
        fill
        options={[
          { value: "route", label: t("Route"), hint: t("rule sets, ready-made sets and MATCH") },
          {
            value: "custom",
            label: t("Custom rules"),
            hint: t("{n} above everything else", { n: routing.rules.length }),
          },
        ]}
        value={page}
        onChange={(next) => setPage(next as Page)}
      />

      {page === "route" ? (
        <>
          <Text tone="muted" size="xs" className="block">
            {t(
              "Checked top to bottom, the first match wins: your rules → high → medium → low → MATCH. On one level, rule sets go first.",
            )}
          </Text>
          <RuleSets
            entries={routing.ruleSets}
            onChange={(ruleSets) => commit({ ...routing, ruleSets })}
            targets={targets}
            nodes={nodes}
            lost={lost}
            onMessage={onMessage}
          />
          <ReadySets
            ready={routing.ready}
            onChange={(ready) => commit({ ...routing, ready })}
            targets={targets}
            nodes={nodes}
            lost={lost}
            onMessage={onMessage}
          />

          {/* Build-in — то, что клиент ставит сам (D-158): дно маршрута и базы ядра. */}
          <Divider label={t("build-in — what the client adds itself")} />
          {/* MATCH — дно списка, а не его строка: не двигается и не удаляется. */}
          <Card
            padding="sm"
            iconTone={fallback.tone}
            icon={<CornerDownRight />}
            title="MATCH"
            description={t(
              "everything not matched above · umiray is the exit chosen in Connection",
            )}
            actions={
              <div className="w-[230px]">
                <TargetPicker
                  value={routing.fallback}
                  groups={targets}
                  nodes={nodes}
                  lost={lost}
                  label={t("Where to send everything else")}
                  onChange={(fallback) => commit({ ...routing, fallback })}
                />
              </div>
            }
          />
          {/* Базы ядра — не строка маршрута: отдельно от build-in, последними. */}
          <Divider label={t("core databases")} />
          <GeoBases running={running} onMessage={onMessage} />
        </>
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-2">
            <ChipGroup
              single
              size="sm"
              aria-label={t("Filter rules by target")}
              value={[filter]}
              onChange={(next) => setFilter(next[0] ?? "all")}
              options={[
                { value: "all", label: t("All"), count: routing.rules.length },
                ...seen.map((id) => ({
                  value: id,
                  label: id,
                  icon: (() => {
                    const { Icon } = lost(id) ? lostLook() : targetLook(id, nodes);
                    return <Icon />;
                  })(),
                  count: routing.rules.filter((rule) => rule.target === id).length,
                })),
              ]}
            />
            <span className="flex-1" />
            <Text tone="muted" size="xs" className="block">
              {filter !== "all"
                ? t("only → {target}; order and numbering are shared", { target: filter })
                : t("checked before rule sets, top to bottom; the first match wins")}
            </Text>
          </div>

          {shown.map((row) => (
            <RuleRow
              // Ключ по месту: правила не имеют своего имени, а перестановка меняет место.
              key={row.index}
              rule={row.rule}
              no={row.index + 1}
              kinds={kinds}
              targets={targets}
              nodes={nodes}
              lost={lost}
              lists={lists}
              first={row.index === 0}
              last={row.index === routing.rules.length - 1}
              open={expanded === row.index}
              onToggle={() => setExpanded((was) => (was === row.index ? null : row.index))}
              onChange={(rule) =>
                commit({
                  ...routing,
                  rules: routing.rules.map((item, at) => (at === row.index ? rule : item)),
                })
              }
              onMove={(delta) => move(row.index, delta)}
              onRemove={() => {
                setExpanded((was) =>
                  was === null || was < row.index ? was : was === row.index ? null : was - 1,
                );
                commit({
                  ...routing,
                  rules: routing.rules.filter((_, at) => at !== row.index),
                });
              }}
            />
          ))}

          {routing.rules.length === 0 && (
            <EmptyState
              size="sm"
              title={t("No custom rules")}
              hint={t("Everything goes by the route: rule sets, ready-made sets and MATCH.")}
            />
          )}

          <Button
            className="self-start"
            icon={<Plus />}
            onClick={() => {
              setFilter("all");
              setExpanded(routing.rules.length);
              commit({
                ...routing,
                rules: [
                  ...routing.rules,
                  { kind: KINDS[0], values: [], target: targets[0], options: [] },
                ],
              });
            }}
          >
            {t("Rule")}
          </Button>
        </>
      )}
    </div>
  );
}
