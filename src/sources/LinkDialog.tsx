import { Plus } from "lucide-react";
import { useState } from "react";
import { Button, Dialog, Field, Input } from "rootik";
import * as api from "../api";
import { t } from "../i18n";
import { failure } from "../shell/Banner";

type Props = {
  onDone: (result: api.Import) => void;
  onClose: () => void;
  qd?: (link: string) => Promise<void>;
};

/**
 * Add a subscription or a link to one server (D-120). A refusal shows right here, at the
 * field being edited; success closes the dialog.
 */
export default function LinkDialog({ onDone, onClose, qd }: Props) {
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);

  const add = async () => {
    setBusy(true);
    setFailed(null);
    try {
      if (qd) await qd(input.trim());
      else onDone(await api.sourcesAdd(input.trim()));
    } catch (e) {
      setFailed(failure(e).text);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open
      size="sm"
      title={t("Link")}
      description={
        qd
          ? t("One qd:// link from your provider. It carries the entry nodes and the network key.")
          : t("A subscription address from your provider or a link to one server.")
      }
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("Cancel")}
          </Button>
          <Button
            variant="primary"
            icon={<Plus />}
            loading={busy}
            disabled={!input.trim()}
            onClick={add}
          >
            {t("Add")}
          </Button>
        </>
      }
    >
      <Field label={qd ? t("Link") : t("Subscription address")} error={failed ?? undefined}>
        <Input
          type={qd ? "text" : "url"}
          mono
          value={input}
          placeholder={qd ? "qd://…" : t("https://… or vless://…")}
          spellCheck={false}
          onChange={(event) => setInput(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && input.trim() && !busy) add();
          }}
        />
      </Field>
    </Dialog>
  );
}
