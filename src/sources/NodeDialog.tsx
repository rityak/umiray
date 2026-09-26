import { Code2, Plus, SlidersHorizontal } from "lucide-react";
import { useState } from "react";
import { Button, Callout, Dialog, Spacer, Text } from "rootik";
import * as api from "../api";
import Editor from "../config/Editor";
import { t } from "../i18n";
import ProxyForm from "./ProxyForm";
import { missing, PROTOCOLS, toEntry, type Values } from "./proxy";

type Props = {
  onDone: (result: api.Import) => void;
  onClose: () => void;
  onFailed: (error: unknown) => void;
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
export default function NodeDialog({ onDone, onClose, onFailed }: Props) {
  const [kind, setKind] = useState(PROTOCOLS[0].id);
  const [values, setValues] = useState<Values>({});
  const [text, setText] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const gaps = missing(kind, values).map((label) => t(label));
  const coding = text !== null;

  /// Code starts from what is already typed: an empty code view would mean typing it all again.
  const toCode = async () => {
    setBusy(true);
    try {
      setText(await api.sourcesProxyYaml(toEntry(kind, values)));
    } catch (e) {
      onFailed(e);
    } finally {
      setBusy(false);
    }
  };

  const add = async () => {
    setBusy(true);
    try {
      onDone(
        coding
          ? await api.sourcesAddProxyText(text)
          : await api.sourcesAddProxy(toEntry(kind, values)),
      );
    } catch (e) {
      onFailed(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open
      size="lg"
      title={t("Custom node")}
      description={t("Fields are named the way mihomo names them. Empty ones are not written.")}
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
            disabled={!coding && gaps.length > 0}
            title={
              !coding && gaps.length > 0
                ? t("Missing: {fields}", { fields: gaps.join(", ") })
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
        {coding ? (
          <>
            <div className="h-96 overflow-hidden">
              <Editor value={text} onChange={setText} />
            </div>
            <Text tone="muted" size="xs" className="block">
              {t(
                "What is written here will be added. You can go back to the fields, but what you typed in code is lost then — the form assembles the node again.",
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
                {t("The node will not come up without: {fields}.", { fields: gaps.join(", ") })}
              </Callout>
            )}
            <Text tone="muted" size="xs" className="block">
              {t(
                "The core has more fields than shown here — add the rest in code: a node added this way is editable as a whole config.",
              )}
            </Text>
          </>
        )}
      </div>
    </Dialog>
  );
}
