import { FilePlus2, Plus, Rss } from "lucide-react";
import { useState } from "react";
import { Button, Callout, Field, Input, InputGroup, Item, ItemGroup, Text } from "rootik";
import type * as api from "../api";
import { t, tn } from "../i18n";
import { failure, type Message } from "../shell/Banner";

type Props = {
  sources: api.Source[];
  /// Ссылка qd принята или ждёт прав (D-161).
  qd: boolean;
  /// Любая ссылка — mihomo или `qd://`; отказ бросается и показывается у поля.
  onLink: (input: string) => Promise<Message>;
  /// `null` — окно выбора файла закрыли.
  onFile: () => Promise<Message | null>;
  onElevate: () => void;
};

/// Шаг мастера «Подписка» (D-162): ссылка, подписка, файл или `qd://` — из одного поля.
export default function SourceStep({ sources, qd, onLink, onFile, onElevate }: Props) {
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  const [said, setSaid] = useState<Message | null>(null);

  const run = async (action: () => Promise<Message | null>) => {
    setBusy(true);
    setFailed(null);
    try {
      const message = await action();
      if (message) {
        setSaid(message);
        setInput("");
      }
    } catch (e) {
      setFailed(failure(e).text);
    } finally {
      setBusy(false);
    }
  };

  const add = () => run(() => onLink(input.trim()));

  return (
    <div className="flex flex-col gap-3">
      <Field
        label={t("Subscription or link")}
        hint={t("https://, vless://, hysteria2://… or qd:// — several can be added")}
        error={failed ?? undefined}
      >
        <InputGroup block>
          <Input
            type="url"
            mono
            aria-label={t("Subscription or link")}
            value={input}
            placeholder={t("https://… or vless://…")}
            spellCheck={false}
            onChange={(event) => setInput(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && input.trim() && !busy) add();
            }}
          />
          <Button
            variant="primary"
            icon={<Plus />}
            loading={busy}
            disabled={!input.trim()}
            onClick={add}
          >
            {t("Add")}
          </Button>
        </InputGroup>
      </Field>
      <div>
        <Button variant="ghost" icon={<FilePlus2 />} disabled={busy} onClick={() => run(onFile)}>
          {t("From file")}
        </Button>
      </div>

      {said && (
        <Callout
          tone="info"
          title={said.text}
          actions={
            said.kind === "needsElevation" && (
              <Button size="sm" variant="primary" onClick={onElevate}>
                {t("Restart as admin")}
              </Button>
            )
          }
        >
          {said.details.length > 0 && (
            <ul className="selectable m-0 list-disc pl-4">
              {said.details.map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
          )}
        </Callout>
      )}

      {sources.length > 0 || qd ? (
        <ItemGroup variant="divided">
          {sources.map((source) => (
            <Item
              key={source.id}
              size="sm"
              icon={<Rss />}
              title={source.name}
              description={tn(source.nodes, "{n} node", "{n} nodes")}
            />
          ))}
          {qd && <Item size="sm" icon={<Rss />} title="qd" description={t("qd link")} />}
        </ItemGroup>
      ) : (
        <Text tone="muted" size="xs">
          {t("No sources yet. You can skip this step and add them later with +.")}
        </Text>
      )}
    </div>
  );
}
