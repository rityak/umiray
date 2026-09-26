import { Plus, Save, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import {
  Badge,
  Button,
  Card,
  Code,
  ConfirmButton,
  Disclosure,
  Input,
  Spinner,
  Switch,
  Text,
} from "rootik";
import * as api from "../api";
import { t, tn } from "../i18n";
import { failure, type Message } from "../shell/Banner";
import CodeMirror from "./CodeMirror";

type Props = {
  /// The list is visible. While it is not, nothing is re-read.
  active: boolean;
  /// Toggling and saving reach the live core by a reload (D-102).
  onStatus: (status: api.Status) => void;
  onMessage: (message: Message) => void;
};

/**
 * Built-in rule sets as switches (D-083). Expanding opens an editor of the same file in
 * `collections/rules/` (D-104); "Save" applies it too.
 */
export default function RulesetsList({ active, onStatus, onMessage }: Props) {
  const [sets, setSets] = useState<api.Ruleset[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [shown, setShown] = useState<string | null>(null);
  /// The draft of the expanded set. One: only one is ever open.
  const [draft, setDraft] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  /// The new set's name. `null` — there is no field at all.
  const [naming, setNaming] = useState<string | null>(null);

  const open = async (set: api.Ruleset, next: boolean) => {
    if (!next) {
      if (shown === set.id) {
        setShown(null);
        setDraft(null);
      }
      return;
    }
    setShown(set.id);
    setDraft(null);
    try {
      setDraft(await api.rulesetsRead(set.id));
    } catch (e) {
      onMessage(failure(e));
      setShown(null);
    }
  };

  const save = async (set: api.Ruleset) => {
    if (draft === null) return;
    setSaving(true);
    try {
      onStatus(await api.rulesetsWrite(set.id, draft));
      setSets(await api.rulesetsList());
    } catch (e) {
      onMessage(failure(e));
    } finally {
      setSaving(false);
    }
  };

  useEffect(() => {
    if (!active) return;
    api.rulesetsList().then(setSets, () => setSets([]));
  }, [active]);

  /// Created and expanded at once: a set with an example rule exists to be rewritten.
  const create = async () => {
    const title = (naming ?? "").trim();
    if (!title) return;
    try {
      const id = await api.rulesetsCreate(title);
      setSets(await api.rulesetsList());
      setNaming(null);
      setShown(id);
      setDraft(await api.rulesetsRead(id));
    } catch (e) {
      onMessage(failure(e));
    }
  };

  const remove = async (set: api.Ruleset) => {
    setBusy(set.id);
    try {
      onStatus(await api.rulesetsDelete(set.id));
      setSets(await api.rulesetsList());
      if (shown === set.id) {
        setShown(null);
        setDraft(null);
      }
    } catch (e) {
      onMessage(failure(e));
    } finally {
      setBusy(null);
    }
  };

  const toggle = async (set: api.Ruleset) => {
    setBusy(set.id);
    try {
      onStatus(await api.rulesetsSet(set.id, !set.on));
      setSets(await api.rulesetsList());
    } catch (e) {
      onMessage(failure(e));
    } finally {
      setBusy(null);
    }
  };

  const adder =
    naming === null ? (
      <Button className="self-start" icon={<Plus />} onClick={() => setNaming("")}>
        {t("Custom set")}
      </Button>
    ) : (
      <div className="flex items-center gap-1.5">
        <Input
          autoFocus
          className="min-w-0 flex-1"
          aria-label={t("New set name")}
          placeholder={t("Set name")}
          value={naming}
          onChange={(event) => setNaming(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") create();
            if (event.key === "Escape") setNaming(null);
          }}
        />
        <Button disabled={naming.trim() === ""} onClick={create}>
          {t("Create")}
        </Button>
      </div>
    );

  if (sets.length === 0) {
    return (
      <div className="flex flex-col gap-2">
        <Text tone="muted" size="xs" className="block">
          {t("The folder")} <Code>collections/rules</Code>{" "}
          {t("is empty. A set is a file with the fields")} <Code>title</Code> {t("and")}{" "}
          <Code>rules</Code>.
        </Text>
        {adder}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      {sets.map((set) => (
        <Card key={set.id} padding="sm" variant={set.on ? "default" : "outline"}>
          <div className="flex items-center gap-2.5">
            <Switch
              className="min-w-0 flex-1"
              label={set.title}
              description={`${set.id} · ${tn(set.rules.length, "{n} rule", "{n} rules")}`}
              checked={set.on}
              disabled={busy !== null}
              onChange={() => toggle(set)}
            />
            {busy === set.id && (
              <Badge tone="warn" dot>
                {t("applying…")}
              </Badge>
            )}
            <ConfirmButton
              size="sm"
              variant="ghost"
              icon={<Trash2 />}
              disabled={busy !== null}
              confirmLabel={t("Delete for sure?")}
              aria-label={t("Delete set {title}", { title: set.title })}
              onConfirm={() => remove(set)}
            >
              {t("Delete")}
            </ConfirmButton>
          </div>
          <Disclosure
            title={t("What is inside")}
            open={shown === set.id}
            onToggle={(event) => open(set, event.currentTarget.open)}
          >
            <div className="flex flex-col gap-1.5">
              <div className="h-52 overflow-hidden">
                {draft === null ? (
                  <Spinner label={t("Reading the file")} />
                ) : (
                  <CodeMirror value={draft} onChange={setDraft} />
                )}
              </div>
              <div className="flex items-center justify-end gap-2">
                <Code className="min-w-0 flex-1 truncate">collections/rules/{set.id}.yaml</Code>
                <Button
                  size="sm"
                  icon={<Save />}
                  loading={saving}
                  disabled={draft === null || busy !== null}
                  onClick={() => save(set)}
                >
                  {t("Save")}
                </Button>
              </div>
            </div>
          </Disclosure>
        </Card>
      ))}
      {adder}
    </div>
  );
}
