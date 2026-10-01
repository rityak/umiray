import { Check, Ellipsis, Pencil, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import {
  Badge,
  Button,
  IconButton,
  Input,
  Menu,
  MenuItem,
  MenuSeparator,
  Select,
  Tooltip,
} from "rootik";
import type * as api from "../api";
import { t } from "../i18n";

type Props = {
  /// The section's documents — which are the presets (D-071).
  docs: api.ConfigDoc[];
  value: string;
  onChange: (id: string) => void;
  /// A preset has an unsaved edit: in a list of ten this is the only way to see where it is.
  dirty: (id: string) => boolean;
  onApply: () => void;
  onCreate: () => void;
  onRename: (name: string) => void;
  onDelete: () => void;
};

/**
 * Picking a routing preset (D-075).
 *
 * There can be several presets, and routing follows exactly one — the one in use (D-071,
 * D-166). Any of them can be viewed and edited: that is how a preset is prepared
 * without touching the working route. So next to the list there is one of two things: an
 * "in use" badge or a "Use" button — and nothing else. Rename, create and delete are rare
 * actions; they live in the "⋯" menu, not as four icons in the bar.
 */
export default function PresetPicker({
  docs,
  value,
  onChange,
  dirty,
  onApply,
  onCreate,
  onRename,
  onDelete,
}: Props) {
  const [name, setName] = useState<string | null>(null);
  /// Deleting takes the written rules with it — a second press in the same menu confirms.
  const [armed, setArmed] = useState(false);

  const doc = docs.find((item) => item.id === value) ?? docs[0];
  if (doc === undefined) return null;

  const rename = () => {
    const next = (name ?? "").trim();
    setName(null);
    if (next !== "" && next !== doc.label) onRename(next);
  };

  return (
    <>
      {name === null ? (
        <Select
          className="w-[220px]"
          aria-label={t("Routing preset")}
          value={doc.id}
          onChange={onChange}
          options={docs.map((item) => ({
            value: item.id,
            label: `${item.label}${dirty(item.id) ? " •" : ""}`,
            hint: item.applied ? t("in use") : undefined,
          }))}
        />
      ) : (
        // Renaming in place: the field takes the list's spot.
        <Input
          className="w-[220px]"
          aria-label={t("Preset name")}
          autoFocus
          value={name}
          onChange={(event) => setName(event.target.value)}
          onBlur={rename}
          onKeyDown={(event) => {
            if (event.key === "Enter") event.currentTarget.blur();
            if (event.key === "Escape") setName(null);
          }}
        />
      )}
      <Menu
        onOpenChange={(open) => !open && setArmed(false)}
        trigger={<IconButton variant="ghost" icon={<Ellipsis />} label={t("Preset actions")} />}
      >
        <MenuItem icon={<Pencil />} onSelect={() => setName(doc.label)}>
          {t("Rename")}
        </MenuItem>
        <MenuItem
          icon={<Plus />}
          hint={t("a copy of what the client builds itself")}
          onSelect={onCreate}
        >
          {t("New preset")}
        </MenuItem>
        <MenuSeparator />
        <MenuItem
          icon={<Trash2 />}
          danger
          keepOpen={!armed}
          onSelect={() => (armed ? onDelete() : setArmed(true))}
        >
          {armed ? t("Delete for sure?") : t("Delete preset")}
        </MenuItem>
      </Menu>
      {doc.applied ? (
        <Badge tone="success" icon={<Check />}>
          {t("in use")}
        </Badge>
      ) : (
        <Tooltip content={t("Turns routing on and follows this preset")}>
          <Button onClick={onApply}>{t("Use")}</Button>
        </Tooltip>
      )}
    </>
  );
}
