import type { LucideIcon } from "lucide-react";
import { useState } from "react";
import {
  Card,
  Field,
  NavGroup,
  NavItem,
  Nest,
  SettingsGroup,
  SettingsRow,
  Text,
  useScrollSpy,
} from "rootik";
import { t } from "../i18n";
import Scroll from "../shell/Scroll";

/// One setting. An empty `label` puts the control at full width: that is how lists and
/// tables sit, for which a caption on the left only gets in the way.
export type Setting = {
  id: string;
  label?: string;
  /// What it means. Always shown, as text rather than a hover hint (D-117).
  hint?: React.ReactNode;
  /// Control to the right of the caption (checkboxes, switches). Without it — under the caption.
  inline?: boolean;
  control: React.ReactNode;
  /// This option's parameters (D-131). Empty — the option is off and they are hidden.
  nested?: Setting[];
};

/// Second level: "about what exactly" — Windows, WireGuard, DNS.
export type Part = { id: string; label: string; hint?: string; settings: Setting[] };

/// First level: "what about" — Startup, Anti-DPI, Capture. No deeper nesting (D-117).
export type Group = { id: string; label: string; icon: LucideIcon; parts: Part[] };

type Props = {
  groups: Group[];
  /// The section's bar (`SectionBar`): view, document, actions and what the document is.
  bar: React.ReactNode;
};

/**
 * The settings page: a table of contents on the left, setting groups on the right (D-117).
 * The contents list every part at once and highlight the one being read.
 *
 * Both columns are their own cards with their own scroll: the window page does not scroll
 * (STYLEGUIDE, "Frame"), and the contents stay put while the settings scroll.
 */
export default function Page({ groups, bar }: Props) {
  // The highlight follows the settings column's scroll, not the window's: the column scrolls.
  const [root, setRoot] = useState<HTMLElement | null>(null);
  const ids = groups.flatMap((group) => group.parts.map((part) => part.id));
  const active = useScrollSpy(ids, { root, offset: 120 });

  return (
    <>
      {bar}
      <div className="grid min-h-0 flex-1 grid-cols-[200px_minmax(0,1fr)] grid-rows-[minmax(0,1fr)] gap-3 max-[820px]:grid-cols-1">
        <Card padding="sm" className="min-h-0 max-[820px]:hidden">
          <div className="flex h-full min-h-0 flex-col gap-2 overflow-y-auto">
            {/* The same look as the utility list in Tools: a group as a heading, a part
                as an item with its group's icon. Two different tables of contents in
                neighbouring sections read as two different apps. */}
            <nav aria-label={t("Settings sections")} className="flex flex-col gap-1">
              {groups.map((group) => (
                <NavGroup key={group.id} label={group.label}>
                  {group.parts.map((part) => (
                    <NavItem
                      key={part.id}
                      icon={<group.icon />}
                      label={part.label}
                      active={active === part.id}
                      onClick={() =>
                        document
                          .getElementById(part.id)
                          ?.scrollIntoView({ block: "start", behavior: "smooth" })
                      }
                    />
                  ))}
                </NavGroup>
              ))}
            </nav>
          </div>
        </Card>
        <Scroll ref={setRoot}>
          {groups.map((group) => (
            <div key={group.id} id={group.id} className="flex scroll-mt-2 flex-col gap-3">
              {group.parts.map((part, index) => (
                <SettingsGroup
                  key={part.id}
                  id={part.id}
                  data-anchor={part.id}
                  className="scroll-mt-2"
                  title={index === 0 ? `${group.label} · ${part.label}` : part.label}
                  description={part.hint}
                >
                  <Rows items={part.settings} />
                </SettingsGroup>
              ))}
            </div>
          ))}
        </Scroll>
      </div>
    </>
  );
}

/// Setting rows. Separate because a block with its own state (handshake masking) draws
/// the same rows.
export function Rows({ items }: { items: Setting[] }) {
  return (
    <>
      {items.map((setting) => (
        <Row key={setting.id} setting={setting} />
      ))}
    </>
  );
}

function Row({ setting }: { setting: Setting }) {
  const { label, hint, control, inline, nested } = setting;
  const sub =
    nested && nested.length > 0 ? (
      <Nest>
        <Rows items={nested} />
      </Nest>
    ) : undefined;
  if (inline && label) {
    return (
      <SettingsRow data-setting={setting.id} label={label} hint={hint} nested={sub}>
        {control}
      </SettingsRow>
    );
  }
  // A wide control (lists, tables, card choices) goes under the caption at full width.
  return (
    <div data-setting={setting.id} className="flex flex-col gap-1.5 py-2">
      {label ? (
        <Field label={label} hint={hint}>
          {control}
        </Field>
      ) : (
        <>
          {hint && (
            <Text tone="muted" size="xs" className="block">
              {hint}
            </Text>
          )}
          {control}
        </>
      )}
      {sub}
    </div>
  );
}
