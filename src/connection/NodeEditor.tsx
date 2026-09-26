import { Code2, RotateCcw, Save, SlidersHorizontal, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Button, Callout, ConfirmButton, Dialog, Spacer, Text, Tooltip } from "rootik";
import * as api from "../api";
import Editor from "../config/Editor";
import { t } from "../i18n";
import { failure, type Message } from "../shell/Banner";
import ProxyForm from "../sources/ProxyForm";
import { type Entry, fromEntry, missing, toEntry, type Values } from "../sources/proxy";

type Props = {
  node: api.Node;
  onClose: () => void;
  onChanged: () => void;
  onMessage: (message: Message) => void;
};

/// Шапка кода у узла, который клиент написал сам. Её же признак — что узел можно убрать:
/// заводили его здесь, а не подпиской.
const MINE = "# Ваш узел";

/**
 * Узел: та же форма, что при добавлении, только заполненная (D-121).
 *
 * Запись, которую пишет клиент (свой узел, файл, `wireguard://` — D-063), приезжает
 * объектом, раскладывается моделью протокола и правится теми же полями, какими её
 * собирали. Новый протокол заводится в `proxy.ts` и появляется здесь сам.
 *
 * **Где формы быть не может, на её месте стоит причина, а не пустая панель.** Ссылку
 * читает ядро (D-031), своего YAML туда не положить: остаются те правки, которые
 * доезжают параметром ссылки (D-114), и код только для чтения.
 *
 * Чего модель не знает, форма не теряет: незнакомые поля переживают правку, и окно
 * пересчитывает их вслух — молчащая потеря хуже отказа.
 */
export default function NodeEditor({ node, onClose, onChanged, onMessage }: Props) {
  const [code, setCode] = useState<api.NodeCode | null>(null);
  const [text, setText] = useState<string | null>(null);
  const [values, setValues] = useState<Values>({});
  const [extra, setExtra] = useState<Entry>({});
  const [coding, setCoding] = useState(false);
  const [saving, setSaving] = useState(false);
  const entry = code?.entry ?? null;
  const form = entry !== null;
  const kind = String(entry?.type ?? node.kind.toLowerCase());
  const mine = form && (code?.text.startsWith(MINE) ?? false);

  const load = useCallback(
    () =>
      api.nodesCode(node.source, node.name).then(
        (next) => {
          setCode(next);
          setText(next.text);
          if (next.entry !== null) {
            const parsed = fromEntry(next.entry);
            setValues(parsed.values);
            setExtra(parsed.extra);
          }
        },
        (e) => onMessage(failure(e)),
      ),
    [node.source, node.name, onMessage],
  );

  // Черновик заводится на **узел**, а не на каждый его приезд: список опрашивается
  // раз в секунду и приносит новый объект, и зависимость от него стирала бы правку.
  // biome-ignore lint/correctness/useExhaustiveDependencies: узел опознаётся парой «источник + имя»
  useEffect(() => {
    setCoding(false);
    setCode(null);
    setText(null);
    setValues({});
    setExtra({});
    load();
  }, [node.source, node.name]);

  const built = form ? toEntry(kind, values, extra) : null;
  const gaps = form ? missing(kind, values) : [];
  const recoded = code !== null && text !== null && code.editable && text !== code.text;
  const reshaped = built !== null && JSON.stringify(built) !== JSON.stringify(entry);
  const dirty = recoded || reshaped;
  const leftover = Object.keys(extra);

  /// Переход в код показывает то, что уедет ядру, — с несохранённой правкой поверх.
  const show = async (next: boolean) => {
    setSaving(true);
    try {
      if (built !== null && next) setText(await api.sourcesProxyYaml(built));
      setCoding(next);
    } catch (e) {
      onMessage(failure(e));
    } finally {
      setSaving(false);
    }
  };

  /// Одно действие на три: сохранить, откатить к присланному, убрать свой узел.
  const apply = async (what: "save" | "rollback" | "delete") => {
    setSaving(true);
    try {
      if (what === "delete") {
        await api.nodesDelete(node.source, node.name);
      } else if (what === "rollback") {
        await api.nodesReset(node.source, node.name);
      } else {
        // Из кода едет текст, из формы — объект: обе дороги ведут в одно хранилище.
        if (coding && recoded && text !== null) {
          await api.nodesCodeSet(node.source, node.name, text);
        } else if (built !== null && reshaped) {
          await api.nodesEntrySet(node.source, node.name, built);
        }
      }
      onChanged();
      onClose();
    } catch (e) {
      onMessage(failure(e));
    } finally {
      setSaving(false);
    }
  };

  const showingCode = coding || (!form && text !== null);

  return (
    <Dialog
      open
      size={form ? "lg" : "md"}
      title={node.name}
      description={`${node.kind} · ${node.address ?? t("address unknown")}${node.edited ? ` · ${t("edited")}` : ""}`}
      onClose={onClose}
      footer={
        <>
          <Button
            variant="ghost"
            size="sm"
            disabled={saving}
            data-view={showingCode ? "code" : "visual"}
            icon={showingCode ? <SlidersHorizontal /> : <Code2 />}
            onClick={() => show(!coding)}
          >
            {showingCode ? t("Fields") : t("Code")}
          </Button>
          <Spacer />
          {mine && (
            <ConfirmButton
              variant="danger"
              size="sm"
              disabled={saving}
              icon={<Trash2 />}
              confirmLabel={t("Remove for sure?")}
              onConfirm={() => apply("delete")}
            >
              {t("Remove")}
            </ConfirmButton>
          )}
          <Tooltip content={t("Restore the node exactly as supplied by its source")}>
            <Button
              size="sm"
              disabled={saving || !node.edited}
              icon={<RotateCcw />}
              onClick={() => apply("rollback")}
            >
              {t("Revert")}
            </Button>
          </Tooltip>
          <Tooltip
            content={
              gaps.length > 0
                ? t("Missing: {fields}", { fields: gaps.map((field) => t(field)).join(", ") })
                : undefined
            }
          >
            <Button
              variant="primary"
              size="sm"
              loading={saving}
              disabled={!dirty || (!coding && gaps.length > 0)}
              icon={<Save />}
              onClick={() => apply("save")}
            >
              {t("Save")}
            </Button>
          </Tooltip>
        </>
      }
    >
      <div className="flex flex-col gap-2">
        {showingCode ? (
          <>
            <div
              data-code={code === null ? "loading" : code.editable ? "editable" : "read-only"}
              className="h-96 overflow-hidden"
            >
              {text === null ? (
                <Text tone="muted" size="xs" className="block">
                  {t("Reading node config…")}
                </Text>
              ) : (
                <Editor value={text} readOnly={!code?.editable} onChange={setText} />
              )}
            </div>
            <Text tone="muted" size="xs" className="block">
              {code?.editable
                ? t(
                    "Edit the whole config here. Only overrides are stored, so fresh subscription keys still arrive after your edits.",
                  )
                : (code?.why ?? t("Read-only."))}
            </Text>
          </>
        ) : form ? (
          <>
            <ProxyForm
              kind={kind}
              values={values}
              locked={["name"]}
              why={t(
                "groups, rules and routing identify this node by name — it cannot be renamed here",
              )}
              onChange={(key, value) => setValues((was) => ({ ...was, [key]: value }))}
            />
            {gaps.length > 0 && (
              <Callout tone="warn">
                {t("The node will not come up without: {fields}.", {
                  fields: gaps.map((field) => t(field)).join(", "),
                })}
              </Callout>
            )}
            {/* Сколько полей форма не знает — вслух. Молчащая потеря хуже отказа (D-121). */}
            <Text tone="muted" size="xs" className="block">
              {leftover.length > 0
                ? t(
                    "{n} fields are not shown: {fields}. They are preserved as-is and can be edited in Code view.",
                    { n: leftover.length, fields: leftover.join(", ") },
                  )
                : t("All existing fields are shown. Add other settings in Code view.")}
            </Text>
          </>
        ) : (
          // Формы нет — вместо неё причина, а не пустая панель (D-121, D-122).
          <Callout tone="neutral">{code?.why ?? t("Reading node…")}</Callout>
        )}
      </div>
    </Dialog>
  );
}
