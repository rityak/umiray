import { RotateCcw, Save, Undo2 } from "lucide-react";
import { useEffect, useRef } from "react";
import { Button, ConfirmButton, IconButton, Tooltip } from "rootik";
import { t } from "../i18n";

type Props = {
  dirty: boolean;
  busy?: boolean;
  onUndo: () => void;
  onSave: () => void;
  /// Reset to default. Absent — no button: the default for Groups is an empty document,
  /// and a reset would mean "delete all my groups at once".
  onReset?: () => void;
};

/**
 * Actions on a document — the same in every section, in one order and one size: reset,
 * revert, save. "Unsaved" is no longer a separate badge: the button itself says so — it is
 * enabled — and so does the dot on the dock tab.
 *
 * Ctrl+S lives here too: wherever there is "Save", the shortcut works. We listen on the
 * window, not the bar — while editing, focus sits inside CodeMirror.
 */
export default function SaveActions({ dirty, busy = false, onUndo, onSave, onReset }: Props) {
  const save = useRef(onSave);
  save.current = dirty && !busy ? onSave : () => {};
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== "s") return;
      event.preventDefault();
      save.current();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <>
      {onReset && (
        <ConfirmButton
          variant="ghost"
          icon={<RotateCcw />}
          aria-label={t("Reset to default")}
          confirmLabel={t("Reset for sure?")}
          onConfirm={onReset}
        />
      )}
      <IconButton
        variant="ghost"
        icon={<Undo2 />}
        label={t("Revert unsaved changes")}
        disabled={!dirty || busy}
        onClick={onUndo}
      />
      <Tooltip content={t("Save")} shortcut="Ctrl+S">
        <Button variant="primary" icon={<Save />} loading={busy} disabled={!dirty} onClick={onSave}>
          {t("Save")}
        </Button>
      </Tooltip>
    </>
  );
}
