import {
  AppWindow,
  ArrowRight,
  ArrowUpRight,
  CornerDownRight,
  Download,
  Ellipsis,
  Globe,
  type LucideIcon,
  Plane,
  Plus,
  ShieldCheck,
  Trash2,
  Upload,
} from "lucide-react";
import { useCallback, useRef, useState } from "react";
import {
  Badge,
  Button,
  Card,
  ChipGroup,
  Divider,
  EmptyState,
  IconButton,
  Menu,
  MenuItem,
  Select,
  Stat,
  Text,
  type Tone,
  Tooltip,
} from "rootik";
import { useCached } from "../hooks/useCached";
import { unchanged, usePoll } from "../hooks/usePoll";
import { t, tk } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";
import ProcessPicker from "../shell/ProcessPicker";
import Scroll from "../shell/Scroll";
import SectionBar from "../shell/SectionBar";
import * as qd from "./api";

type Props = {
  live: boolean;
  onMessage: (message: Message) => void;
};

const LOOK: Record<qd.Role, { label: string; word: string; tone: Tone; Icon: LucideIcon }> = {
  direct: { label: "direct", word: tk("bypass VPN"), tone: "neutral", Icon: ArrowUpRight },
  tunnel: { label: "tunnel", word: tk("through VPN"), tone: "accent", Icon: Globe },
  egress: { label: "+egress", word: tk("through the exit node"), tone: "accent", Icon: Plane },
  noEgress: {
    label: "-egress",
    word: tk("through VPN, never the exit node"),
    tone: "accent",
    Icon: ShieldCheck,
  },
};

const plain = (role: qd.Role) => role === "direct" || role === "tunnel";

/// qd's rules: app → role, MATCH holds the default (QD.md). Looks like the mihomo rules on purpose.
// ponytail: the frame (summary, filter chips, numbered cards, MATCH) is a copy of RulesForm's —
// the rule models differ. Lift it into a shared list when a third engine brings rules of its own.
export default function QdRouting({ live, onMessage }: Props) {
  const [routing, setRouting] = useCached<qd.Routing | null>("qd.routing", null);
  const [picking, setPicking] = useState(false);
  const [filter, setFilter] = useState<string>("all");
  const [armed, setArmed] = useState<number | null>(null);
  const generation = useRef(0);

  const load = useCallback(() => {
    const asked = generation.current;
    qd.routing().then(
      (got) => {
        if (asked === generation.current) unchanged(setRouting)(got);
      },
      () => {},
    );
  }, []);
  usePoll(load, live);

  const save = useCallback(
    async (defaultRole: qd.Role, rules: qd.Rule[]) => {
      const mine = ++generation.current;
      setRouting((was) => was && { ...was, defaultRole, rules });
      try {
        const saved = await qd.saveRouting(defaultRole, rules);
        if (mine === generation.current) setRouting(saved);
      } catch (e) {
        onMessage(failure(e));
        generation.current++;
        load();
      }
    },
    [onMessage, load],
  );

  const doExport = async () => {
    try {
      const path = await qd.exportRules();
      if (path) onMessage(notice(t("Rules saved: {path}", { path })));
    } catch (e) {
      onMessage(failure(e));
    }
  };

  const doImport = async () => {
    try {
      const loaded = await qd.importRules();
      if (!loaded) return;
      onMessage(notice(t("Loaded {count} rules", { count: loaded.rules })));
      generation.current++;
      load();
    } catch (e) {
      onMessage(failure(e));
    }
  };

  const bar = (
    <SectionBar
      end={
        <>
          <Button icon={<Download />} disabled={!routing} onClick={doExport}>
            {t("Save to file")}
          </Button>
          <Button icon={<Upload />} disabled={!routing} onClick={doImport}>
            {t("Load from file")}
          </Button>
        </>
      }
      hint={t(
        "Rules pick apps by process. An app without a rule goes to MATCH, together with traffic Windows sends on its own behalf.",
      )}
    />
  );

  if (!routing) {
    return (
      <>
        {bar}
        <Scroll>
          <Card>
            <EmptyState title={t("qd is not running")} hint={t("Routing loads once qd is up.")} />
          </Card>
        </Scroll>
      </>
    );
  }

  const { defaultRole, allowExit, rules } = routing;
  const roles = (current: qd.Role) =>
    qd.ROLES.filter((role) => allowExit || plain(role) || role === current).map((role) => ({
      value: role,
      label: LOOK[role].label,
      hint: t(LOOK[role].word),
      icon: (() => {
        const { Icon } = LOOK[role];
        return <Icon />;
      })(),
    }));
  const taken = new Set(rules.map((rule) => (rule.path || rule.process).toLowerCase()));
  const seen = qd.ROLES.filter((role) => rules.some((rule) => rule.role === role));
  const shown = rules
    .map((rule, index) => ({ rule, index }))
    .filter((row) => filter === "all" || row.rule.role === filter);
  const fallback = LOOK[plain(defaultRole) ? defaultRole : "tunnel"];

  const add = ({ process, path }: { process: string; path?: string }) => {
    if (taken.has((path || process).toLowerCase())) return;
    const role: qd.Role = defaultRole === "direct" ? "tunnel" : "direct";
    setFilter("all");
    save(defaultRole, [...rules, { id: 0, process, path, role }]);
  };

  return (
    <>
      {bar}
      <Scroll>
        <div className="flex flex-col gap-3">
          <Card padding="sm">
            <div className="grid grid-cols-4 gap-3 max-[640px]:grid-cols-2">
              <Stat size="sm" label={t("Rules")} value={rules.length} />
              <Stat
                size="sm"
                label={t("Through VPN")}
                value={rules.filter((rule) => rule.role !== "direct").length}
              />
              <Stat
                size="sm"
                label={t("Bypass VPN")}
                value={rules.filter((rule) => rule.role === "direct").length}
              />
              {allowExit && (
                <Stat
                  size="sm"
                  label={t("Through the exit node")}
                  value={rules.filter((rule) => rule.role === "egress").length}
                />
              )}
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
                { value: "all", label: t("All"), count: rules.length },
                ...seen.map((role) => ({
                  value: role,
                  label: LOOK[role].label,
                  icon: (() => {
                    const { Icon } = LOOK[role];
                    return <Icon />;
                  })(),
                  count: rules.filter((rule) => rule.role === role).length,
                })),
              ]}
            />
            <span className="flex-1" />
            {rules.length === 0 && (
              <Text tone="muted" size="xs" className="block">
                {t("no rules — all traffic goes to MATCH below")}
              </Text>
            )}
          </div>

          {shown.map(({ rule, index }) => {
            const look = LOOK[rule.role];
            const no = index + 1;
            return (
              <Card key={rule.id} padding="sm" className="um-rule">
                <div className="um-rule-header grid grid-cols-[24px_minmax(0,1fr)_16px_210px_32px] items-center gap-2">
                  <Tooltip content={t(look.word)}>
                    <span className="um-no" data-tone={look.tone}>
                      {no}
                    </span>
                  </Tooltip>
                  <div className="flex min-w-0 items-center gap-2">
                    {rule.icon ? (
                      <img src={rule.icon} alt="" width={20} height={20} />
                    ) : (
                      <AppWindow size={20} aria-hidden="true" />
                    )}
                    <div className="min-w-0">
                      <Text size="sm" truncate className="block">
                        {rule.process}
                      </Text>
                      {rule.path && (
                        <Text tone="muted" size="xs" truncate className="block">
                          {rule.path}
                        </Text>
                      )}
                    </div>
                  </div>
                  <ArrowRight aria-hidden="true" className="um-rule-arrow" size={14} />
                  <Select
                    aria-label={t("Where to send, rule {no}", { no })}
                    value={rule.role}
                    options={roles(rule.role)}
                    onChange={(role) =>
                      save(
                        defaultRole,
                        rules.map((item) => (item.id === rule.id ? { ...item, role } : item)),
                      )
                    }
                  />
                  <Menu
                    onOpenChange={(next) => !next && setArmed(null)}
                    trigger={
                      <IconButton
                        size="sm"
                        variant="ghost"
                        icon={<Ellipsis />}
                        label={t("Rule {no} actions", { no })}
                      />
                    }
                  >
                    <MenuItem
                      danger
                      icon={<Trash2 />}
                      keepOpen={armed !== rule.id}
                      onSelect={() =>
                        armed === rule.id
                          ? save(
                              defaultRole,
                              rules.filter((item) => item.id !== rule.id),
                            )
                          : setArmed(rule.id)
                      }
                    >
                      {armed === rule.id ? t("Delete this rule?") : t("Delete rule {no}", { no })}
                    </MenuItem>
                  </Menu>
                </div>
                {!allowExit && !plain(rule.role) && (
                  <Badge size="sm" tone="warn" className="self-start">
                    {t("no exit — follows the default")}
                  </Badge>
                )}
              </Card>
            );
          })}

          <Button className="self-start" icon={<Plus />} onClick={() => setPicking(true)}>
            {t("Rule")}
          </Button>
          {picking && (
            <ProcessPicker
              taken={taken}
              onPick={add}
              onClose={() => setPicking(false)}
              load={qd.processes}
              cacheKey="qd.processes"
              title={t("New rule")}
              searchLabel={t("Search by name or path")}
            />
          )}

          <Divider label={t("below your rules — MATCH")} />

          <Card
            padding="sm"
            iconTone={fallback.tone}
            icon={<CornerDownRight />}
            title="MATCH"
            description={t("apps without a rule and traffic Windows sends on its own behalf")}
            actions={
              <div className="w-[230px]">
                <Select
                  aria-label={t("Where to send everything else")}
                  value={plain(defaultRole) ? defaultRole : "tunnel"}
                  options={(["direct", "tunnel"] as const).map((role) => ({
                    value: role,
                    label: LOOK[role].label,
                    hint: t(LOOK[role].word),
                    icon: (() => {
                      const { Icon } = LOOK[role];
                      return <Icon />;
                    })(),
                  }))}
                  onChange={(role) => save(role, rules)}
                />
              </div>
            }
          />
        </div>
      </Scroll>
    </>
  );
}
