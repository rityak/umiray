import { CornerDownRight, Plus } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Button, Card, ChipGroup, Divider, EmptyState, Stat, Text } from "rootik";
import * as api from "../api";
import { useCached } from "../hooks/useCached";
import { t } from "../i18n";
import { failure, type Message } from "../shell/Banner";
import BuiltinRules from "./BuiltinRules";
import { targetLook } from "./kinds";
import RuleRow from "./RuleRow";
import TargetPicker from "./TargetPicker";

type Props = {
  /// Черновик документа — тот же, что открыт в коде (D-074).
  text: string;
  onDraft: (text: string) => void;
  onMessage: (message: Message | null) => void;
  onPending: (pending: boolean) => void;
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

const EMPTY: api.Routing = { rules: [], fallback: "umiray" };

let kept: { text: string; routing: api.Routing } | null = null;

/// Список без повторов, порядок первого появления.
function unique(items: string[]): string[] {
  return items.filter((item, at) => items.indexOf(item) === at);
}

/**
 * Раздел «Маршрутизация» формой.
 *
 * Порядок здесь — это и есть смысл: побеждает первое совпавшее сверху. Поэтому фишки
 * назначений сверху **фильтруют**, а не перекладывают: номера остаются общими для всего
 * списка, и по ним видно, что выше и что ниже спрятанного.
 */
export default function RulesForm({ text, onDraft, onMessage, onPending }: Props) {
  const [routing, setRouting] = useState<api.Routing>(() =>
    kept?.text === text ? kept.routing : EMPTY,
  );
  kept = { text, routing };
  const [groups, setGroups] = useCached<string[]>("rules.groups", []);
  /// Узлы как второй список «куда» (D-082). Спрашиваем у бэкенда, а не считаем из групп:
  /// состав узлов знает каталог источников, а не документ.
  const [nodes, setNodes] = useCached<string[]>("rules.nodes", []);
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
        () => setGroups([]),
      );
  }, []);

  useEffect(() => {
    // Узел, которого ядро не поднимет, целью быть не может (D-063): группа под него
    // получилась бы пустой, а такую ядро не принимает.
    api.nodesList().then(
      (list) => setNodes(list.filter((node) => node.supported).map((node) => node.name)),
      () => setNodes([]),
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
          hint={`${refused} ${t("Open Code view to edit the original document without losing anything.")}`}
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
  ]).filter((target) => !nodes.includes(target));
  const kinds = unique([...KINDS, ...routing.rules.map((rule) => rule.kind)]);
  const lines = routing.rules.reduce((sum, rule) => sum + rule.values.length, 0);
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

  const through = routing.rules.filter(
    (rule) => targetLook(rule.target, nodes).tone !== "neutral" && rule.target !== "REJECT",
  ).length;
  const past = routing.rules.filter((rule) => rule.target === "DIRECT").length;
  const blocked = routing.rules.filter((rule) => rule.target === "REJECT").length;
  const fallback = targetLook(routing.fallback, nodes);

  return (
    <div className="flex flex-col gap-3">
      {/* Сводка: сколько правил куда ведёт — ответ на «что вообще настроено» одним взглядом. */}
      <Card padding="sm">
        <div className="grid grid-cols-4 gap-3 max-[640px]:grid-cols-2">
          <Stat
            size="sm"
            label={t("Rules")}
            value={routing.rules.length}
            hint={t("config lines: {n}", { n: lines })}
          />
          <Stat size="sm" label={t("Through VPN")} value={through} />
          <Stat size="sm" label={t("Bypass VPN")} value={past} />
          <Stat size="sm" label={t("Block")} value={blocked} />
        </div>
      </Card>

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
                const { Icon } = targetLook(id, nodes);
                return <Icon />;
              })(),
              count: routing.rules.filter((rule) => rule.target === id).length,
            })),
          ]}
        />
        <span className="flex-1" />
        {/* «Сверху вниз» уже сказано в полосе набора; здесь — только то, что зависит от фильтра. */}
        {(routing.rules.length === 0 || filter !== "all") && (
          <Text tone="muted" size="xs" className="block">
            {routing.rules.length === 0
              ? t("no rules — all traffic goes to MATCH below")
              : t("only → {target}; order and numbering are shared", { target: filter })}
          </Text>
        )}
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

      {/* Готовые наборы стоят там, куда попадут: ниже ваших правил и выше MATCH (D-083). */}
      <Divider label={t("below your rules — built-in sets and MATCH")} />
      <BuiltinRules />

      {/* MATCH — дно списка, а не его строка: не двигается и не удаляется. */}
      <Card
        padding="sm"
        iconTone={fallback.tone}
        icon={<CornerDownRight />}
        title="MATCH"
        description={t("everything that did not match above")}
        actions={
          <div className="w-[230px]">
            <TargetPicker
              value={routing.fallback}
              groups={targets}
              nodes={nodes}
              label={t("Where to send everything else")}
              onChange={(fallback) => commit({ ...routing, fallback })}
            />
          </div>
        }
      />
    </div>
  );
}
