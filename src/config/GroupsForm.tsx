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
};

/// Stable keys survive group renaming and empty origins on newly added groups.
type Row = { key: number; group: api.Group; selection: Selection };

const EMPTY: Choices = { sources: [], nodes: [] };

let kept: { text: string; rows: Row[] } | null = null;

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
export default function GroupsForm({ text, onDraft, onMessage }: Props) {
  const [rows, setRows] = useState<Row[]>(() => (kept?.text === text ? kept.rows : []));
  const [choices, setChoices] = useCached<Choices>("groups.choices", EMPTY);
  const cached = useRef(kept?.text === text);
  kept = { text, rows };
  const [open, setOpen] = useState<number | null>(null);
  const [refused, setRefused] = useState<string | null>(null);
  const next = useRef(0);
  /// Names and positions change while editing, so neither can identify a row.
  const key = useCallback(() => {
    next.current += 1;
    return next.current;
  }, []);
  /// Origins refer to this document. Rebuilding on our own output after reordering
  /// would attach unknown fields to the wrong group.
  const base = useRef<string | null>(null);
  /// Do not parse the form's own output again.
  const ours = useRef<string | null>(null);

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
    if (cached.current) {
      cached.current = false;
      return;
    }
    let alive = true;
    api.groupsParse(text).then(
      (groups) => {
        if (!alive) return;
        setRefused(null);
        setRows(
          groups.map((group) => ({
            key: key(),
            group,
            selection: read(group, choicesRef.current),
          })),
        );
      },
      (e) => alive && setRefused(api.asAppError(e).message),
    );
    return () => {
      alive = false;
    };
  }, [text, key]);

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
        onDraft(rendered);
      } catch (e) {
        onMessage(failure(e));
      }
    },
    [onDraft, onMessage],
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
          choices={choices}
          open={open === row.key}
          onToggle={() => setOpen(open === row.key ? null : row.key)}
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

      <BuiltinGroups choices={choices} mine={rows.map((row) => row.group.name)} />
    </div>
  );
}
