import { Layers, Plus } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Button, Card, EmptyState } from "rootik";
import * as api from "../api";
import { useCached } from "../hooks/useCached";
import { t } from "../i18n";
import { failure, type Message } from "../shell/Banner";
import BuiltinGroups from "./BuiltinGroups";
import GroupRow from "./GroupRow";
import { type Choices, read, type Selection } from "./groups";

type Props = {
  /// Form and code share one draft and one Save action (D-074).
  text: string;
  onDraft: (text: string) => void;
  onMessage: (message: Message | null) => void;
  /// Значки групп — общие с «Соединением» (`Settings.group_icons`, D-172).
  icons: Record<string, string>;
  onIcons: (icons: Record<string, string>) => void;
};

/// Stable keys survive group renaming and empty origins on newly added groups.
type Row = { key: number; group: api.Group; selection: Selection };

const EMPTY: Choices = { sources: [], nodes: [] };

/// What the form showed when the section was left: the rows, the draft they render to and
/// the document their origins point into. Coming back to the same draft resumes exactly
/// this; taking our own output as the new base would point origins at the wrong groups.
let kept: { text: string; rows: Row[]; base: string } | null = null;

/// Keys are unique per session, not per mount: kept rows outlive the component, and a
/// counter restarting at zero gave a new group the key of the first one — both opened
/// and were edited as one.
let last = 0;
const key = () => {
  last += 1;
  return last;
};

/// "Create" on the Groups tab of Connection (D-172): the next form that opens starts a new
/// group, unfolded. A flag, not a prop: the request crosses sections, the form is mounted later.
let wanted = false;
export function requestNewGroup() {
  wanted = true;
}

function fresh(key: number): Row {
  return {
    key,
    group: {
      name: t("New group"),
      kind: "select",
      sources: [],
      proxies: [],
      filter: null,
      url: null,
      interval: null,
      tolerance: null,
      strategy: null,
      extra: [],
      origin: null,
    },
    selection: { picked: [], substring: "", others: [], understood: true },
  };
}

/**
 * Edits update the shared draft; Save, Revert and Ctrl+S also serve code view.
 */
export default function GroupsForm({ text, onDraft, onMessage, icons, onIcons }: Props) {
  const setIcon = (name: string, id: string | null) => {
    const next = { ...icons };
    if (id === null) delete next[name];
    else next[name] = id;
    onIcons(next);
  };
  const [resumed] = useState(() => (kept?.text === text ? kept : null));
  const [rows, setRows] = useState<Row[]>(() => resumed?.rows ?? []);
  const [choices, setChoices] = useCached<Choices>("groups.choices", EMPTY);
  const [open, setOpen] = useState<number | null>(null);
  const [refused, setRefused] = useState<string | null>(null);
  /// Rows reflect the document: resumed or parsed.
  const [ready, setReady] = useState(resumed !== null);
  /// Origins refer to this document. Rebuilding on our own output after reordering
  /// would attach unknown fields to the wrong group.
  const base = useRef<string | null>(resumed?.base ?? null);
  /// Do not parse the form's own output again.
  const ours = useRef<string | null>(resumed ? text : null);

  /// Parsing may finish before the node catalog arrives; read its latest value.
  const choicesRef = useRef(choices);
  choicesRef.current = choices;

  useEffect(() => {
    Promise.all([api.sourcesList(), api.nodesList()]).then(
      ([sources, nodes]) => setChoices({ sources, nodes }),
      (e) => onMessage(failure(e)),
    );
  }, [onMessage]);

  useEffect(() => {
    // Compare only with our output. Revert restores base and must reparse it.
    if (text === ours.current) return;
    base.current = text;
    let alive = true;
    api.groupsParse(text).then(
      (groups) => {
        if (!alive) return;
        const parsed = groups.map((group) => ({
          key: key(),
          group,
          selection: read(group, choicesRef.current),
        }));
        kept = { text, rows: parsed, base: text };
        setRefused(null);
        setRows(parsed);
        setReady(true);
      },
      (e) => alive && setRefused(api.asAppError(e).message),
    );
    return () => {
      alive = false;
    };
  }, [text]);

  /// Recompute derived selections when the catalog arrives after parsing.
  useEffect(() => {
    setRows((current) => current.map((row) => ({ ...row, selection: read(row.group, choices) })));
  }, [choices]);

  const commit = useCallback(
    async (list: Row[]) => {
      setRows(list);
      try {
        const rendered = await api.groupsRender(
          base.current ?? "",
          list.map((row) => row.group),
        );
        ours.current = rendered;
        kept = { text: rendered, rows: list, base: base.current ?? "" };
        onDraft(rendered);
      } catch (e) {
        onMessage(failure(e));
      }
    },
    [onDraft, onMessage],
  );

  useEffect(() => {
    if (!ready || !wanted) return;
    wanted = false;
    const row = fresh(key());
    setOpen(row.key);
    commit([...rows, row]);
  }, [ready, rows, commit]);

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

  return (
    <div className="flex flex-col gap-2">
      {rows.length === 0 && (
        <EmptyState
          icon={<Layers />}
          title={t("No custom groups")}
          hint={
            choices.sources.length === 0
              ? t("Add a subscription or a link in Sources first.")
              : t(
                  "AUTO already covers every source. Create a group to pick a subset, like one country.",
                )
          }
        />
      )}
      {rows.map((row) => (
        <GroupRow
          key={row.key}
          group={row.group}
          selection={row.selection}
          taken={rows.filter((item) => item.key !== row.key).map((item) => item.group.name.trim())}
          choices={choices}
          open={open === row.key}
          onToggle={() => setOpen(open === row.key ? null : row.key)}
          icon={icons[row.group.name] ?? null}
          onIcon={(id) => setIcon(row.group.name, id)}
          onChange={(group, selection) =>
            commit(
              rows.map((item) => (item.key === row.key ? { ...item, group, selection } : item)),
            )
          }
          onRemove={() => commit(rows.filter((item) => item.key !== row.key))}
        />
      ))}

      <Button
        className="self-start"
        icon={<Plus />}
        onClick={() => {
          const row = fresh(key());
          setOpen(row.key);
          commit([...rows, row]);
        }}
      >
        {t("Group")}
      </Button>

      <BuiltinGroups
        choices={choices}
        mine={rows.map((row) => row.group.name)}
        icons={icons}
        onIcon={setIcon}
      />
    </div>
  );
}
