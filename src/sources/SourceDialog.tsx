import { useState } from "react";
import { Button, Dialog, Field, Input } from "rootik";
import * as api from "../api";
import { t } from "../i18n";
import { failure } from "../shell/Banner";
import { hide } from "../shell/secret";

type Props = {
  source: api.Source;
  hidden: boolean;
  onClose: () => void;
  onSaved: () => void;
};

/// «Настройки источника» (D-172): название — только то, как источник подписан в окне;
/// узлы, адрес и обновление от него не зависят. Ссылка — для справки.
export default function SourceDialog({ source, hidden, onClose, onSaved }: Props) {
  const [name, setName] = useState(source.name);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);

  const save = async () => {
    setBusy(true);
    setFailed(null);
    try {
      await api.sourcesRename(source.id, name);
      onSaved();
      onClose();
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
      title={t("Source settings")}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("Cancel")}
          </Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={!name.trim() || name.trim() === source.name}
            onClick={save}
          >
            {t("Save")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <Field label={t("Name")} error={failed ?? undefined}>
          <Input
            value={name}
            onChange={(event) => setName(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && name.trim() && !busy) save();
            }}
          />
        </Field>
        {source.url !== null && (
          <Field label={t("Link")}>
            <Input mono readOnly value={hide(source.url, hidden) ?? ""} />
          </Field>
        )}
      </div>
    </Dialog>
  );
}
