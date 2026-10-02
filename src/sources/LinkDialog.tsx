import { Plus } from "lucide-react";
import { useState } from "react";
import { Button, Dialog, Field, Input } from "rootik";
import { t } from "../i18n";
import { isLink } from "../qd/api";
import { failure } from "../shell/Banner";

type Props = {
  /// Take the link. A refusal is thrown and shown at the field; the caller closes on success.
  onSubmit: (link: string) => Promise<void>;
  onClose: () => void;
  /// Opened from the qd view: qd takes only its qd:// link (D-161), and a vless:// or a
  /// subscription pasted there would silently land in mihomo.
  qdOnly?: boolean;
};

/**
 * Add a subscription, a link to one server or a qd:// link (D-120, D-161). A refusal shows
 * right here, at the field being edited; success closes the dialog.
 */
export default function LinkDialog({ onSubmit, onClose, qdOnly = false }: Props) {
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);

  const add = async () => {
    if (qdOnly && !isLink(input)) {
      setFailed(t("qd takes only a qd:// link. Add other links on the mihomo view."));
      return;
    }
    setBusy(true);
    setFailed(null);
    try {
      await onSubmit(input.trim());
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
        qdOnly
          ? t("Add the qd:// link from your provider. A new link replaces the current one.")
          : t(
              "A subscription address, a link to one server, or a qd:// link — qd is downloaded if needed.",
            )
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
      <Field label={t("Subscription address")} error={failed ?? undefined}>
        <Input
          type="url"
          mono
          value={input}
          placeholder={qdOnly ? "qd://…" : t("https://… or vless://…")}
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
