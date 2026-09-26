import type { ReactNode } from "react";
import { Checkbox, Fieldset, Input, Field as KitField, Nest, PasswordInput, Select } from "rootik";
import { t } from "../i18n";
import type { Field, Values } from "./proxy";
import { PROTOCOLS, protocol, shown } from "./proxy";

type Props = {
  kind: string;
  values: Values;
  /// Absent — the type cannot change: for an existing node that would be another node, not an edit.
  onKind?: (kind: string) => void;
  onChange: (key: string, value: string) => void;
  /// Fields shown but not editable (an existing node's name, D-121).
  locked?: string[];
  /// Why the field is locked — as a caption under it.
  why?: string;
};

/**
 * Node fields from the protocol model (D-121). One form for two dialogs: "Manually" and the
 * node editor. A new protocol is one entry in `proxy.ts` and shows up here by itself.
 */
export default function ProxyForm({ kind, values, onKind, onChange, locked, why }: Props) {
  const here = new Set(shown(kind, values).map((field) => field.key));
  const visible = (field: Field) => here.has(field.key);
  const shut = new Set(locked ?? []);

  /// A field, and under it the fields that hang off its checkbox (D-131).
  const rows = (fields: Field[], parent?: string): ReactNode[] =>
    fields
      .filter((field) => field.under === parent && visible(field))
      .map((field) => {
        const inner = rows(fields, field.key);
        return (
          <div key={field.key} className="flex flex-col gap-2">
            <Row
              field={field}
              value={values[field.key] ?? ""}
              locked={shut.has(field.key)}
              why={shut.has(field.key) ? why : undefined}
              onChange={(next) => onChange(field.key, next)}
            />
            {inner.length > 0 && (
              <Nest>
                <div className="flex flex-col gap-2">{inner}</div>
              </Nest>
            )}
          </div>
        );
      });

  return (
    <div className="flex flex-col gap-3">
      <KitField label={t("Type")}>
        <Select
          value={kind}
          disabled={onKind === undefined}
          onChange={(value) => onKind?.(value)}
          options={PROTOCOLS.map((item) => ({ value: item.id, label: item.label }))}
        />
      </KitField>
      <div className="grid gap-x-4 gap-y-3 min-[680px]:grid-cols-2">
        {protocol(kind)
          .parts.filter((part) => part.fields.some(visible))
          .map((part) => (
            <Fieldset key={part.title} legend={t(part.title)}>
              {rows(part.fields)}
            </Fieldset>
          ))}
      </div>
    </div>
  );
}

/// The control comes from the field description, not the value: when adding there is nothing to infer from.
function Row({
  field,
  value,
  locked,
  why,
  onChange,
}: {
  field: Field;
  value: string;
  locked?: boolean;
  why?: string;
  onChange: (value: string) => void;
}) {
  const hint = why ?? (field.hint === undefined ? undefined : t(field.hint));
  if (field.kind === "bool") {
    return (
      <Checkbox
        label={t(field.label)}
        description={hint}
        disabled={locked}
        checked={value === "true"}
        onChange={(event) => onChange(event.target.checked ? "true" : "")}
      />
    );
  }
  return (
    <KitField label={t(field.label)} hint={hint} required={field.need}>
      {field.kind === "select" ? (
        <Select
          clearable
          value={value === "" ? null : value}
          disabled={locked}
          onChange={(next) => onChange(next ?? "")}
          options={(field.options ?? []).map((option) => ({ value: option, label: option }))}
        />
      ) : field.kind === "secret" ? (
        <PasswordInput
          value={value}
          disabled={locked}
          spellCheck={false}
          onChange={(event) => onChange(event.target.value)}
        />
      ) : (
        <Input
          type={field.kind === "number" ? "number" : "text"}
          value={value}
          disabled={locked}
          mono={field.kind !== "text"}
          placeholder={
            field.kind === "multi"
              ? (field.options ?? []).join(", ")
              : field.placeholder && t(field.placeholder)
          }
          spellCheck={false}
          onChange={(event) => onChange(event.target.value)}
        />
      )}
    </KitField>
  );
}
