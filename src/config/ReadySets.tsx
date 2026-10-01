import { Package, Plus, Save, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import {
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
import { useCached } from "../hooks/useCached";
import { getLang, t, tn } from "../i18n";
import { failure, type Message } from "../shell/Banner";
import CodeMirror from "./CodeMirror";
import PriorityPicker from "./PriorityPicker";
import TargetPicker from "./TargetPicker";

type Props = {
  /// The route's `ready` from the draft (D-158): which sets are in, and exits overridden.
  ready: api.ReadyUse[];
  onChange: (ready: api.ReadyUse[]) => void;
  /// "Where to" — the same choices a rule has.
  targets: string[];
  nodes: string[];
  onMessage: (message: Message | null) => void;
};

function name(set: api.Ruleset): string {
  return getLang() === "en" ? (set.titleEn ?? set.title) : set.title;
}

/**
 * Ready-made rule sets (D-083, D-158): files in `collections/rules/`. A switch puts a set
 * into this route; its exit is the one its lines share and can be overridden. Expanding
 * opens an editor of the file itself (D-104) — "Save" there writes the file, not the route.
 */
export default function ReadySets({ ready, onChange, targets, nodes, onMessage }: Props) {
  const [sets, setSets] = useCached<api.Ruleset[]>("rulesets", []);
  const [shown, setShown] = useState<string | null>(null);
  /// The draft of the expanded set. One: only one is ever open.
  const [draft, setDraft] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  /// The new set's name. `null` — there is no field at all.
  const [naming, setNaming] = useState<string | null>(null);

  useEffect(() => {
    api.rulesetsList().then(setSets, () => setSets([]));
  }, []);

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
      await api.rulesetsWrite(set.id, draft);
      setSets(await api.rulesetsList());
    } catch (e) {
      onMessage(failure(e));
    } finally {
      setSaving(false);
    }
  };

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

  /// The file goes, and so does its line in this route: a route naming a set that is gone
  /// would be refused on save (D-158).
  const remove = async (set: api.Ruleset) => {
    try {
      await api.rulesetsDelete(set.id);
      setSets(await api.rulesetsList());
      onChange(ready.filter((use) => use.id !== set.id));
      if (shown === set.id) {
        setShown(null);
        setDraft(null);
      }
    } catch (e) {
      onMessage(failure(e));
    }
  };

  const toggle = (set: api.Ruleset, on: boolean) =>
    onChange(on ? [...ready, { id: set.id }] : ready.filter((use) => use.id !== set.id));

  /// The set's own exit is stored as nothing: then a change of the set's lines carries over.
  const aim = (set: api.Ruleset, target: string) =>
    onChange(
      ready.map((use) =>
        use.id === set.id ? { ...use, target: target === set.target ? undefined : target } : use,
      ),
    );

  const rank = (set: api.Ruleset, priority: api.Priority | undefined) =>
    onChange(ready.map((use) => (use.id === set.id ? { ...use, priority } : use)));

  const adder =
    naming === null ? (
      <Button className="self-start" size="sm" icon={<Plus />} onClick={() => setNaming("")}>
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

  return (
    <Card
      headingLevel={3}
      icon={<Package />}
      title={t("Ready-made sets")}
      description={t("ready rules, each with its own exit you can change")}
    >
      <div className="flex flex-col gap-2">
        {sets.length === 0 && (
          <Text tone="muted" size="xs" className="block">
            {t("No ready-made sets yet. A set is a document with the fields")} <Code>title</Code>{" "}
            {t("and")} <Code>rules</Code>.
          </Text>
        )}
        {sets.map((set) => {
          const use = ready.find((item) => item.id === set.id);
          return (
            <Card key={set.id} padding="sm" variant={use ? "default" : "outline"}>
              <div className="flex items-center gap-2.5">
                <Switch
                  className="min-w-0 flex-1"
                  label={name(set)}
                  description={`${set.id} · ${tn(set.rules.length, "{n} rule", "{n} rules")}`}
                  checked={use !== undefined}
                  onChange={() => toggle(set, use === undefined)}
                />
                {use && (
                  <div className="w-[130px]">
                    <PriorityPicker
                      value={use.priority}
                      label={t("Priority of «{name}»", { name: name(set) })}
                      onChange={(priority) => rank(set, priority)}
                    />
                  </div>
                )}
                {use && set.target !== null && (
                  <div className="w-[200px]">
                    <TargetPicker
                      value={use.target ?? set.target}
                      groups={targets}
                      nodes={nodes}
                      label={t("Where to send «{name}»", { name: name(set) })}
                      onChange={(target) => aim(set, target)}
                    />
                  </div>
                )}
                {use && set.target === null && (
                  <Text tone="muted" size="xs">
                    {t("exits are set in its lines")}
                  </Text>
                )}
                <ConfirmButton
                  size="sm"
                  variant="ghost"
                  icon={<Trash2 />}
                  confirmLabel={t("Delete for sure?")}
                  aria-label={t("Delete set {title}", { title: name(set) })}
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
                      <Spinner label={t("Reading the set")} />
                    ) : (
                      <CodeMirror value={draft} onChange={setDraft} />
                    )}
                  </div>
                  <div className="flex items-center justify-end gap-2">
                    <Button
                      size="sm"
                      icon={<Save />}
                      loading={saving}
                      disabled={draft === null}
                      onClick={() => save(set)}
                    >
                      {t("Save")}
                    </Button>
                  </div>
                </div>
              </Disclosure>
            </Card>
          );
        })}
        {adder}
      </div>
    </Card>
  );
}
