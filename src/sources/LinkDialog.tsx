import { Plus } from "lucide-react";
import { useState } from "react";
import { Button, Dialog, Field, Input } from "rootik";
import { t } from "../i18n";
import { failure } from "../shell/Banner";

type Props = {
  /// Take the link. A refusal is thrown and shown at the field; the caller closes on success.
  onSubmit: (link: string) => Promise<void>;
  onClose: () => void;
};

/**
 * Add a subscription, a link to one server or a qd:// link (D-120, D-161). A refusal shows
 * right here, at the field being edited; success closes the dialog.
 */
export default function LinkDialog({ onSubmit, onClose }: Props) {
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);

  const add = async () => {
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
      description={t(
        "A subscription address, a link to one server, or a qd:// link — qd is downloaded if needed.",
      )}
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
          placeholder={t("https://… or vless://…")}
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
