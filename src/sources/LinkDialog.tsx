import { Plus } from "lucide-react";
import { useState } from "react";
import { Button, Dialog, Field, Input } from "rootik";
import { t } from "../i18n";
import { failure } from "../shell/Banner";

/// What kind of link the dialog takes. Every engine has its own (D-154).
export type LinkCopy = {
  description: string;
  label: string;
  placeholder: string;
  type: "url" | "text";
};

type Props = {
  /// Take the link. A refusal is thrown and shown at the field; the caller closes on success.
  onSubmit: (link: string) => Promise<void>;
  onClose: () => void;
  /// A subscription address or a link to one server, unless said otherwise.
  copy?: LinkCopy;
};

/**
 * Add a subscription or a link to one server (D-120). A refusal shows right here, at the
 * field being edited; success closes the dialog.
 */
export default function LinkDialog({ onSubmit, onClose, copy }: Props) {
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  const text = copy ?? {
    description: t("A subscription address from your provider or a link to one server."),
    label: t("Subscription address"),
    placeholder: t("https://… or vless://…"),
    type: "url",
  };

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
      description={text.description}
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
      <Field label={text.label} error={failed ?? undefined}>
        <Input
          type={text.type}
          mono
          value={input}
          placeholder={text.placeholder}
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
