import { Code2, Plus, SlidersHorizontal } from "lucide-react";
import { useState } from "react";
import { Button, Callout, Dialog, Spacer, Text } from "rootik";
import * as api from "../api";
import Editor from "../config/Editor";
import { t } from "../i18n";
import { failure } from "../shell/Banner";
import ProxyForm from "./ProxyForm";
import { missing, PROTOCOLS, toEntry, type Values, wrong } from "./proxy";

type Props = {
  onDone: (result: api.Import) => void;
  onClose: () => void;
};

/**
 * Assemble a node by hand (D-120).
 *
 * The fields are drawn by `ProxyForm` — the same form as in the existing-node editor (D-121):
 * a new protocol is added once, in the model, and works on both sides at once.
 *
 * Next to it is code — the same node as text (D-074). The backend renders it: a second YAML
 * written in the window would drift from the first at the first escape. An edit in code
 * takes over: once you go to text, the text is what gets added, and the window says so.
 */
export default function NodeDialog({ onDone, onClose }: Props) {
  const [kind, setKind] = useState(PROTOCOLS[0].id);
  const [values, setValues] = useState<Values>({});
  const [text, setText] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  /// Отказ — здесь, у полей: баннер лежит под модальным окном, и его не видно.
  const [failed, setFailed] = useState<string | null>(null);

  const gaps = missing(kind, values).map((label) => t(label));
  const bad = wrong(kind, values).map((label) => t(label));
  const coding = text !== null;
  const blocked = !coding && (gaps.length > 0 || bad.length > 0);

  /// Code starts from what is already typed: an empty code view would mean typing it all again.
  const toCode = async () => {
    setBusy(true);
    setFailed(null);
    try {
      setText(await api.sourcesProxyYaml(toEntry(kind, values)));
    } catch (e) {
      setFailed(failure(e).text);
    } finally {
      setBusy(false);
    }
  };

  const add = async () => {
    setBusy(true);
    setFailed(null);
    try {
      onDone(
        coding
          ? await api.sourcesAddProxyText(text)
          : await api.sourcesAddProxy(toEntry(kind, values)),
      );
    } catch (e) {
      setFailed(failure(e).text);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open
      size="lg"
      title={t("Custom node")}
      description={t("Fields use mihomo's names. Empty ones aren't written.")}
      onClose={onClose}
      footer={
        <>
          <Button
            variant="ghost"
            size="sm"
            disabled={busy}
            data-view={coding ? "code" : "visual"}
            icon={coding ? <SlidersHorizontal /> : <Code2 />}
            onClick={() => (coding ? setText(null) : toCode())}
          >
            {coding ? t("Fields") : t("Code")}
          </Button>
          <Spacer />
          <Button variant="ghost" onClick={onClose}>
            {t("Cancel")}
          </Button>
          <Button
            variant="primary"
            icon={<Plus />}
            loading={busy}
            disabled={blocked}
            title={
              !coding && gaps.length > 0
                ? t("Missing: {fields}", { fields: gaps.join(", ") })
                : !coding && bad.length > 0
                  ? t("Check: {fields}", { fields: bad.join(", ") })
                  : undefined
            }
            onClick={add}
          >
            {t("Add")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-2">
        {failed && <Callout tone="danger" title={failed} />}
        {coding ? (
          <>
            <div className="h-96 overflow-hidden">
              <Editor value={text} onChange={setText} />
            </div>
            <Text tone="muted" size="xs" className="block">
              {t(
                "What's written here gets added. Going back to the fields drops what you typed — the form rebuilds the node.",
              )}
            </Text>
          </>
        ) : (
          <>
            <ProxyForm
              kind={kind}
              values={values}
              onKind={setKind}
              onChange={(key, value) => setValues((was) => ({ ...was, [key]: value }))}
            />
            {gaps.length > 0 && (
              <Callout tone="warn">
                {t("The node won't come up without: {fields}.", { fields: gaps.join(", ") })}
              </Callout>
            )}
            {bad.length > 0 && (
              <Callout tone="warn">
                {t("The node won't come up with these values: {fields}.", {
                  fields: bad.join(", "),
                })}
              </Callout>
            )}
            <Text tone="muted" size="xs" className="block">
              {t(
                "The core has more fields than the form — add the rest in code. A node added this way can be edited as a whole config.",
              )}
            </Text>
          </>
        )}
      </div>
    </Dialog>
  );
}
