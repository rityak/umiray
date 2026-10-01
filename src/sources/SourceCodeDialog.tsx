import { useState } from "react";
import { Dialog } from "rootik";
import * as api from "../api";
import type { Drafts } from "../config/draft";
import { t } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";
import SaveActions from "../shell/SaveActions";
import { hide } from "../shell/secret";
import SourceCode from "./SourceCode";

type Props = {
  source: api.Source;
  hidden: boolean;
  /// Черновики сырья живут у `App` (D-138): закрытое окно не стирает несохранённое.
  drafts: Drafts;
  onDraft: (id: string, text: string) => void;
  onDisk: (id: string, text: string) => void;
  onChanged: () => Promise<void>;
  onMessage: (message: Message) => void;
  onClose: () => void;
};

/// Что прислала панель, текстом (D-065) — окном из карточки источника (D-160).
export default function SourceCodeDialog({
  source,
  hidden,
  drafts,
  onDraft,
  onDisk,
  onChanged,
  onMessage,
  onClose,
}: Props) {
  const draft = drafts[source.id];
  const [saving, setSaving] = useState(false);

  const save = async () => {
    if (draft === undefined) return;
    setSaving(true);
    try {
      const result = await api.sourcesWrite(source.id, draft.text);
      onDisk(source.id, draft.text);
      await onChanged();
      onMessage(notice(api.importSummary(result), result.notices));
    } catch (e) {
      onMessage(failure(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog
      open
      size="xl"
      title={hide(source.name, hidden)}
      description={t(
        "The provider's response as is. Names in your edits are cleaned up the same way a refresh does.",
      )}
      onClose={onClose}
      footer={
        draft !== undefined &&
        !hidden && (
          <SaveActions
            dirty={draft.text !== draft.saved}
            busy={saving}
            onUndo={() => onDraft(source.id, draft.saved)}
            onSave={save}
          />
        )
      }
    >
      <div className="flex h-[60vh] min-h-0 flex-col">
        <SourceCode
          source={source}
          hidden={hidden}
          draft={draft}
          onDraft={onDraft}
          onDisk={onDisk}
          onMessage={onMessage}
        />
      </div>
    </Dialog>
  );
}
