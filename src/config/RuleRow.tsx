import {
  AppWindow,
  ArrowDown,
  ArrowRight,
  ArrowUp,
  ChevronDown,
  ChevronUp,
  Ellipsis,
  Plus,
  Trash2,
} from "lucide-react";
import { useEffect, useId, useState } from "react";
import {
  Button,
  Card,
  Checkbox,
  Field,
  IconButton,
  Menu,
  MenuItem,
  MenuSeparator,
  Select,
  Text,
  Textarea,
  Tooltip,
} from "rootik";
import * as api from "../api";
import { t, tn } from "../i18n";
import ProcessPicker from "../shell/ProcessPicker";
import { targetLook } from "./kinds";
import { resolves, retyped, ruleValues, withNoResolve } from "./rule-values";
import TargetPicker from "./TargetPicker";

/// The value field's placeholder depends on the rule kind: "github.com" and "192.168.0.0/16"
/// are written differently, and one shared example helps neither.
const PLACEHOLDER: Record<string, string> = {
  "DOMAIN-SUFFIX": "example.org\nexample.net",
  DOMAIN: "www.example.org",
  "DOMAIN-KEYWORD": "example",
  "IP-CIDR": "192.168.0.0/16",
  GEOIP: "RU",
  "PROCESS-NAME": "python.exe\nnode.exe",
  "DOMAIN-REGEX": "^example[0-9]{1,3}\\.org$",
  "RULE-SET": "antizapret",
};

function valuesLabel(kind: string): string {
  if (kind.includes("REGEX")) return t("Expressions");
  if (kind.startsWith("DOMAIN")) return t("Domains");
  if (kind.startsWith("PROCESS")) return t("Processes");
  if (kind.includes("CIDR")) return t("IP ranges");
  if (kind === "GEOIP" || kind === "GEOSITE") return t("Countries and categories");
  if (kind === "RULE-SET") return t("Rule sets");
  return t("Values");
}

function valuesCount(kind: string, n: number): string {
  if (kind.includes("REGEX")) return tn(n, "{n} expression", "{n} expressions");
  if (kind.startsWith("DOMAIN")) return tn(n, "{n} domain", "{n} domains");
  if (kind.startsWith("PROCESS")) return tn(n, "{n} process", "{n} processes");
  if (kind === "RULE-SET") return tn(n, "{n} list", "{n} lists");
  return tn(n, "{n} value", "{n} values");
}

type Props = {
  rule: api.Rule;
  /// Index in the **whole** list: the filter hides rows but does not renumber them.
  no: number;
  kinds: string[];
  targets: string[];
  nodes: string[];
  /// Downloaded rule sets (D-157): a `RULE-SET` rule offers them instead of making you
  /// remember the name.
  lists: string[];
  first: boolean;
  last: boolean;
  open: boolean;
  onToggle: () => void;
  onChange: (rule: api.Rule) => void;
  onMove: (delta: -1 | 1) => void;
  onRemove: () => void;
};

/**
 * A compact routing summary with a full-width editor on demand (D-148).
 * Each value becomes its own core rule; the global document owns saving.
 */
export default function RuleRow({
  rule,
  no,
  kinds,
  targets,
  nodes,
  lists,
  first,
  last,
  open,
  onToggle,
  onChange,
  onMove,
  onRemove,
}: Props) {
  const look = targetLook(rule.target, nodes);
  const panelId = useId();
  const [text, setText] = useState(() => rule.values.join("\n"));
  const [armed, setArmed] = useState(false);
  // D-153: PROCESS-NAME can take a running exe while free-form values remain editable.
  const [picking, setPicking] = useState(false);
  // Preserve blank lines and the caret while typing; synchronize external undo/moves.
  useEffect(() => {
    setText((was) =>
      JSON.stringify(ruleValues(was)) === JSON.stringify(rule.values)
        ? was
        : rule.values.join("\n"),
    );
  }, [rule.values]);
  const preview = rule.values.slice(0, 2).join(" · ");
  return (
    <Card padding="sm" className="um-rule">
      <div className="um-rule-header grid grid-cols-[24px_170px_minmax(0,1fr)_16px_210px_32px_32px] items-center gap-2">
        {/* The number in the outcome's tone: order and "where" read at a glance. */}
        <Tooltip content={look.word}>
          <span className="um-no" data-tone={look.tone}>
            {no}
          </span>
        </Tooltip>
        <Select
          aria-label={t("Rule {no} kind", { no })}
          value={rule.kind}
          onChange={(kind) => onChange(retyped(rule, kind))}
          options={kinds.map((kind) => ({ value: kind, label: kind }))}
        />
        <button
          type="button"
          className="um-rule-summary min-w-0 text-left"
          aria-expanded={open}
          aria-controls={panelId}
          onClick={onToggle}
        >
          <Text size="sm" className="block">
            {valuesCount(rule.kind, rule.values.length)}
          </Text>
          {!open && (
            <Text tone="muted" size="xs" truncate className="block">
              {preview || t("Add values")}
              {rule.values.length > 2 ? ` · ${t("+{n} more", { n: rule.values.length - 2 })}` : ""}
            </Text>
          )}
        </button>
        <ArrowRight aria-hidden="true" className="um-rule-arrow" size={14} />
        <TargetPicker
          value={rule.target}
          groups={targets}
          nodes={nodes}
          label={t("Where to send, rule {no}", { no })}
          onChange={(target) => onChange({ ...rule, target })}
        />
        <IconButton
          size="sm"
          variant="ghost"
          icon={open ? <ChevronUp /> : <ChevronDown />}
          label={open ? t("Collapse rule {no}", { no }) : t("Edit rule {no} values", { no })}
          aria-expanded={open}
          aria-controls={panelId}
          onClick={onToggle}
        />
        <Menu
          onOpenChange={(next) => !next && setArmed(false)}
          trigger={
            <IconButton
              size="sm"
              variant="ghost"
              data-focus={`${no}:menu`}
              icon={<Ellipsis />}
              label={t("Rule {no} actions", { no })}
            />
          }
        >
          <MenuItem icon={<ArrowUp />} disabled={first} onSelect={() => onMove(-1)}>
            {t("Move rule {no} up", { no })}
          </MenuItem>
          <MenuItem icon={<ArrowDown />} disabled={last} onSelect={() => onMove(1)}>
            {t("Move rule {no} down", { no })}
          </MenuItem>
          <MenuSeparator />
          <MenuItem
            danger
            icon={<Trash2 />}
            keepOpen={!armed}
            onSelect={() => (armed ? onRemove() : setArmed(true))}
          >
            {armed ? t("Delete this rule?") : t("Delete rule {no}", { no })}
          </MenuItem>
        </Menu>
      </div>
      <div id={panelId} hidden={!open}>
        {open && (
          <Field
            label={valuesLabel(rule.kind)}
            hint={t("One value per line. You can paste a whole list.")}
          >
            <div className="flex flex-col items-start gap-2">
              <Textarea
                className="um-rule-values w-full resize-y"
                mono
                rows={6}
                spellCheck={false}
                aria-label={t("Rule {no} values", { no })}
                placeholder={PLACEHOLDER[rule.kind] ?? "example"}
                value={text}
                onChange={(event) => {
                  const next = event.target.value;
                  setText(next);
                  onChange({ ...rule, values: ruleValues(next) });
                }}
              />
              {resolves(rule.kind) && (
                <Checkbox
                  label="no-resolve"
                  description={t(
                    "don't look up a site's address just for this rule: a connection that came with a name skips it and goes to the next rule",
                  )}
                  checked={rule.options.includes("no-resolve")}
                  onChange={(event) =>
                    onChange({
                      ...rule,
                      options: withNoResolve(rule.options, event.target.checked),
                    })
                  }
                />
              )}
              {rule.kind === "PROCESS-NAME" && (
                <Button size="sm" icon={<AppWindow />} onClick={() => setPicking(true)}>
                  {t("Choose running process")}
                </Button>
              )}
              {rule.kind === "RULE-SET" && (
                <div className="flex flex-wrap gap-1">
                  {lists
                    .filter((id) => !rule.values.includes(id))
                    .map((id) => (
                      <Button
                        key={id}
                        size="sm"
                        variant="ghost"
                        icon={<Plus />}
                        onClick={() => {
                          const next = [...ruleValues(text), id];
                          setText(next.join("\n"));
                          onChange({ ...rule, values: next });
                        }}
                      >
                        {id}
                      </Button>
                    ))}
                </div>
              )}
            </div>
          </Field>
        )}
      </div>
      {picking && rule.kind === "PROCESS-NAME" && (
        <ProcessPicker
          taken={new Set(rule.values.map((value) => value.toLowerCase()))}
          onPick={({ process }) => {
            const values = ruleValues(text);
            if (values.some((value) => value.toLowerCase() === process.toLowerCase())) return;
            const next = [...values, process];
            setText(next.join("\n"));
            onChange({ ...rule, values: next });
          }}
          onClose={() => setPicking(false)}
          load={api.rulesProcesses}
          cacheKey="rules.processes"
          title={t("Choose running process")}
          searchLabel={t("Search by name")}
        />
      )}
    </Card>
  );
}
