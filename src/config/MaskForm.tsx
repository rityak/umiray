import { Dices, Save, Undo2 } from "lucide-react";
import { useEffect, useState } from "react";
import { Button, Code, Field, NumberInput, Switch, Text } from "rootik";
import * as api from "../api";
import { useCached } from "../hooks/useCached";
import { t } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";
import { Rows, type Setting } from "./Page";

type Props = {
  onUnsavedChange: (dirty: boolean) => void;
  onMessage: (message: Message | null) => void;
  /// Refresh the shared document after saving the mask (D-052).
  onSaved: () => void;
};

/// One 40–70 byte junk packet was sufficient in measurements (S-023).
const JUNK_ON = { jc: 1, jmin: 40, jmax: 70 };

/// Zero leaves the corresponding AmneziaWG field untouched.
const AWG_OFF = { s1: 0, s2: 0, s3: 0, s4: 0, h1: 0, h2: 0, h3: 0, h4: 0 };

/// Use four distinct headers above WireGuard's reserved 1–4, within u32 range.
const randomHeaders = () => {
  const seen = new Set<number>();
  while (seen.size < 4) seen.add(5 + Math.floor(Math.random() * 2_000_000_000));
  const [h1, h2, h3, h4] = [...seen];
  return { h1, h2, h3, h4 };
};

/**
 * WireGuard handshake obfuscation (D-118). Junk packets work with vanilla servers;
 * packet format changes require AmneziaWG and have a separate opt-in.
 * Save explicitly so typing 40 never writes an intermediate jmin of 4.
 */
export default function MaskForm({ onMessage, onSaved, onUnsavedChange }: Props) {
  const [disk, setDisk] = useCached<api.Mask | null>("mask.disk", null);
  const [draft, setDraft] = useCached<api.Mask | null>("mask.draft", null);
  const [busy, setBusy] = useState(false);
  /// Keep the opt-in visible before numbers are entered; disk stores the fields (D-131).
  const [amnezia, setAmnezia] = useState(false);

  // An unsaved edit outlives leaving the section: the fresh file replaces only an untouched
  // draft. `disk` is the snapshot the cached draft was made against — read once, on opening.
  // biome-ignore lint/correctness/useExhaustiveDependencies: snapshot at opening
  useEffect(() => {
    api.clientMaskGet().then(
      (mask) => {
        setDraft((was) =>
          was !== null &&
          disk !== null &&
          api.MASK_FIELDS.some((field) => was[field] !== disk[field])
            ? was
            : mask,
        );
        setDisk(mask);
      },
      (e) => onMessage(failure(e)),
    );
  }, [onMessage]);

  const dirty =
    draft !== null &&
    disk !== null &&
    api.MASK_FIELDS.some((field) => draft[field] !== disk[field]);
  useEffect(() => {
    onUnsavedChange(dirty);
    return () => onUnsavedChange(false);
  }, [dirty, onUnsavedChange]);

  if (draft === null || disk === null) {
    return (
      <Text tone="muted" size="xs" className="block">
        {t("Reading settings…")}
      </Text>
    );
  }

  const edit = (patch: Partial<api.Mask>) => setDraft({ ...draft, ...patch });
  const junk = draft.jc > 0;
  const expert = ["s1", "s2", "s3", "s4", "h1", "h2", "h3", "h4"] as const;
  const awg = amnezia || expert.some((name) => draft[name] !== 0);

  const save = async () => {
    setBusy(true);
    onMessage(null);
    try {
      const written = await api.clientMaskSet(draft);
      setDisk(written);
      setDraft(written);
      onSaved();
      onMessage(notice(t("Saved. Applies on the next connection.")));
    } catch (e) {
      onMessage(failure(e));
    } finally {
      setBusy(false);
    }
  };

  /// Empty fields mean zero, leaving the value untouched.
  const field = (name: (typeof api.MASK_FIELDS)[number]) => (
    <NumberInput
      className="w-28"
      allowEmpty
      min={0}
      value={draft[name] === 0 ? null : draft[name]}
      disabled={busy}
      placeholder="0"
      aria-label={name}
      onChange={(value) => edit({ [name]: value ?? 0 })}
    />
  );

  const settings: Setting[] = [
    {
      id: "mask-junk",
      label: t("Junk packets before handshake"),
      inline: true,
      hint: t("works with any WireGuard server — no server changes needed"),
      control: (
        <Switch
          aria-label={t("Junk packets before handshake")}
          checked={junk}
          disabled={busy}
          onChange={(event) => edit(event.target.checked ? JUNK_ON : { jc: 0 })}
        />
      ),
      nested: junk
        ? [
            {
              id: "mask-jc",
              label: t("Packet count (jc)"),
              hint: t("one is enough; more adds traffic on every reconnection"),
              control: field("jc"),
            },
            {
              id: "mask-jsize",
              label: t("Junk size, bytes (jmin / jmax)"),
              hint: t("random size within the range; a fixed size would become a fingerprint"),
              control: (
                <span className="flex items-center gap-2">
                  {field("jmin")}…{field("jmax")}
                </span>
              ),
            },
          ]
        : undefined,
    },
    {
      // Packet format changes require an AmneziaWG server (D-131).
      id: "mask-awg",
      label: t("AmneziaWG server"),
      inline: true,
      hint: t(
        "changes packet formats: plain WireGuard can't read them and the tunnel won't come up",
      ),
      control: (
        <Switch
          aria-label={t("AmneziaWG server")}
          checked={awg}
          disabled={busy}
          onChange={(event) => {
            setAmnezia(event.target.checked);
            if (!event.target.checked) edit(AWG_OFF);
          }}
        />
      ),
      nested: awg
        ? [
            {
              id: "mask-awg-fields",
              control: (
                <>
                  <div className="grid grid-cols-2 gap-x-4 gap-y-2 min-[560px]:grid-cols-4">
                    {expert.map((name) => (
                      <Field key={name} label={name.toUpperCase()}>
                        {field(name)}
                      </Field>
                    ))}
                  </div>
                  <Text tone="muted" size="xs" className="block">
                    <Code>s1…s4</Code> — {t("bytes added to each packet.")} <Code>h1…h4</Code> —{" "}
                    {t("packet tags: all four must be distinct numbers greater than four.")}
                  </Text>
                  <Button
                    className="self-start"
                    icon={<Dices />}
                    disabled={busy}
                    onClick={() => edit(randomHeaders())}
                  >
                    {t("Generate headers")}
                  </Button>
                </>
              ),
            },
          ]
        : undefined,
    },
  ];

  return (
    <div className="flex flex-col gap-3.5">
      <Rows items={settings} />

      <div className="flex items-center gap-2">
        <Button icon={<Undo2 />} disabled={!dirty || busy} onClick={() => setDraft(disk)}>
          {t("Revert")}
        </Button>
        <Button variant="primary" icon={<Save />} loading={busy} disabled={!dirty} onClick={save}>
          {t("Apply")}
        </Button>
      </div>
    </div>
  );
}
