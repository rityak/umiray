import { Code2, RotateCcw, Save, SlidersHorizontal, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Button, Callout, ConfirmButton, Dialog, Spacer, Text, Tooltip } from "rootik";
import * as api from "../api";
import Editor from "../config/Editor";
import { t } from "../i18n";
import { failure } from "../shell/Banner";
import ProxyForm from "../sources/ProxyForm";
import {
  type Entry,
  fromEntry,
  missing,
  sameEntry,
  toEntry,
  type Values,
  wrong,
} from "../sources/proxy";

type Props = {
  node: api.Node;
  onClose: () => void;
  onChanged: () => void;
};

/// Шапка кода у узла, который клиент написал сам. Её же признак — что узел можно убрать:
/// заводили его здесь, а не подпиской.
const MINE = "# Ваш узел";

/// По этим полям узел подписки узнаётся (D-036): с другим адресом это другой узел,
/// и бэкенд такую правку отвергнет (B-036). У своего узла они правятся.
const ADDRESS = [
  "server",
  "port",
  "ws-opts.path",
  "h2-opts.path",
  "xhttp-opts.path",
  "grpc-opts.grpc-service-name",
];

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
export default function NodeEditor({ node, onClose, onChanged }: Props) {
  const [code, setCode] = useState<api.NodeCode | null>(null);
  const [text, setText] = useState<string | null>(null);
  /// Текст, с которого начался вид кода. Правка кода — отличие от него, а не от файла:
  /// вход в код показывает собранное формой, и сравнение с файлом считало правкой
  /// уже сам вход.
  const [base, setBase] = useState<string | null>(null);
  /// Отказ показывается в окне: баннер лежит под модальным окном, и его не видно.
  const [failed, setFailed] = useState<string | null>(null);
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
          setBase(next.text);
          if (next.entry !== null) {
            const parsed = fromEntry(next.entry);
            setValues(parsed.values);
            setExtra(parsed.extra);
          }
        },
        (e) => setFailed(failure(e).text),
      ),
    [node.source, node.name],
  );

  // Черновик заводится на **узел**, а не на каждый его приезд: список опрашивается
  // раз в секунду и приносит новый объект, и зависимость от него стирала бы правку.
  // biome-ignore lint/correctness/useExhaustiveDependencies: узел опознаётся парой «источник + имя»
  useEffect(() => {
    setCoding(false);
    setCode(null);
    setText(null);
    setBase(null);
    setFailed(null);
    setValues({});
    setExtra({});
    load();
  }, [node.source, node.name]);

  const built = form ? toEntry(kind, values, extra) : null;
  const gaps = form ? missing(kind, values) : [];
  const bad = form ? wrong(kind, values) : [];
  const recoded = code !== null && text !== null && code.editable && text !== base;
  const reshaped = built !== null && !sameEntry(built, entry);
  const dirty = recoded || reshaped;
  const leftover = Object.keys(extra);

  /// Переход в код показывает то, что уедет ядру, — с несохранённой правкой поверх.
  /// Обратно в поля правка кода не переезжает (YAML разбирает бэкенд): уход из кода с
  /// правкой её сбрасывает, и кнопка об этом спрашивает. Молча она оставалась висеть —
  /// «Сохранить» в полях закрывало окно, ничего не записав.
  const show = async (next: boolean) => {
    setSaving(true);
    setFailed(null);
    try {
      if (built !== null && next) {
        const shown = await api.sourcesProxyYaml(built);
        setText(shown);
        setBase(shown);
      }
      if (!next) setText(base);
      setCoding(next);
    } catch (e) {
      setFailed(failure(e).text);
    } finally {
      setSaving(false);
    }
  };

  /// Одно действие на три: сохранить, откатить к присланному, убрать свой узел.
  const apply = async (what: "save" | "rollback" | "delete") => {
    setSaving(true);
    setFailed(null);
    try {
      if (what === "delete") {
        await api.nodesDelete(node.source, node.name);
      } else if (what === "rollback") {
        await api.nodesReset(node.source, node.name);
      } else {
        // Из кода едет текст, из формы — объект: обе дороги ведут в одно хранилище.
        if (recoded && text !== null) {
          await api.nodesCodeSet(node.source, node.name, text);
        } else if (built !== null && reshaped) {
          await api.nodesEntrySet(node.source, node.name, built);
        }
      }
      onChanged();
      onClose();
    } catch (e) {
      setFailed(failure(e).text);
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
          {/* Без формы переключать не на что: остаётся только код. */}
          {form &&
            (coding && recoded ? (
              <ConfirmButton
                variant="ghost"
                size="sm"
                disabled={saving}
                data-view="code"
                icon={<SlidersHorizontal />}
                confirmLabel={t("Drop code changes?")}
                onConfirm={() => show(false)}
              >
                {t("Fields")}
              </ConfirmButton>
            ) : (
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
            ))}
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
          {/* Откат стирает правки насовсем — как и удаление, вторым нажатием. */}
          <Tooltip content={t("Restore the node as its source sent it")}>
            <ConfirmButton
              size="sm"
              disabled={saving || !node.edited}
              icon={<RotateCcw />}
              confirmLabel={t("Drop your changes?")}
              onConfirm={() => apply("rollback")}
            >
              {t("Revert")}
            </ConfirmButton>
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
              disabled={!dirty || (!coding && (gaps.length > 0 || bad.length > 0))}
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
        {failed && <Callout tone="danger" title={failed} />}
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
                    "Edit the whole config here. Only your changes are stored, so new keys from the subscription still come through.",
                  )
                : (code?.why ?? t("Read-only."))}
            </Text>
          </>
        ) : form ? (
          <>
            <ProxyForm
              kind={kind}
              values={values}
              locked={{
                name: t(
                  "groups, rules and the exit choice find this node by name — it can't be renamed here",
                ),
                ...(mine
                  ? {}
                  : Object.fromEntries(
                      ADDRESS.map((key) => [
                        key,
                        t("another address is another node — add it separately"),
                      ]),
                    )),
              }}
              onChange={(key, value) => setValues((was) => ({ ...was, [key]: value }))}
            />
            {gaps.length > 0 && (
              <Callout tone="warn">
                {t("The node won't come up without: {fields}.", {
                  fields: gaps.map((field) => t(field)).join(", "),
                })}
              </Callout>
            )}
            {bad.length > 0 && (
              <Callout tone="warn">
                {t("The node won't come up with these values: {fields}.", {
                  fields: bad.map((field) => t(field)).join(", "),
                })}
              </Callout>
            )}
            {/* Сколько полей форма не знает — вслух. Молчащая потеря хуже отказа (D-121). */}
            <Text tone="muted" size="xs" className="block">
              {leftover.length > 0
                ? t("{n} fields aren't shown: {fields}. They're kept as is; edit them in Code.", {
                    n: leftover.length,
                    fields: leftover.join(", "),
                  })
                : t("All the node's fields are shown. Add others in Code.")}
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
