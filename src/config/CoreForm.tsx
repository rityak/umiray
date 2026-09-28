import { Cable, Globe, ScrollText } from "lucide-react";
import { useEffect, useState } from "react";
import { ChoiceCards, Code, Input, NumberInput, Select, Spinner, Switch, Textarea } from "rootik";
import * as api from "../api";
import { useCached } from "../hooks/useCached";
import { t, tk } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";
import SaveActions from "../shell/SaveActions";
import SectionBar from "../shell/SectionBar";
import Page, { type Group } from "./Page";

type Props = {
  onMessage: (message: Message | null) => void;
  /// Refresh the shared document after saving through the form (D-052).
  onSaved: () => void;
  /// View and document controls on the left of SectionBar.
  start: React.ReactNode;
  hint: string;
  onUnsavedChange: (dirty: boolean) => void;
};

const STACKS: { id: api.Stack; label: string; hint: string }[] = [
  {
    id: "mixed",
    label: "mixed",
    hint: tk("both stacks — works on most machines; choose this if unsure"),
  },
  {
    id: "system",
    label: "system",
    hint: tk("Windows stack: fastest, but not supported on every machine"),
  },
  {
    id: "gvisor",
    label: "gvisor",
    hint: tk("core stack: broadly compatible, but slightly slower"),
  },
];

const ENHANCED: { id: api.Enhanced; label: string; hint: string }[] = [
  {
    id: "fake-ip",
    label: "fake-ip",
    hint: tk("route by domain name — faster and more precise"),
  },
  {
    id: "redir-host",
    label: "redir-host",
    hint: tk("real addresses — more compatible, but domain rules are less reliable"),
  },
];

const LEVELS: api.LogLevel[] = ["silent", "error", "warning", "info", "debug"];

/// One record per line.
const lines = (list: string[]) => list.join("\n");
const parse = (text: string) => text.split("\n");

/// Convert option metadata into choice cards.
const cards = <T extends string>(items: { id: T; label: string; hint: string }[]) =>
  items.map(({ id, label, hint }) => ({ value: id, label, description: t(hint) }));

/**
 * Edit the supported subset of Mihomo Settings (D-086, D-117).
 * tun.enable belongs to capture controls; mode and profile remain code-only.
 * Save explicitly so partially typed ports and MTUs never reach the core.
 */
export default function CoreForm({ onMessage, onSaved, start, hint, onUnsavedChange }: Props) {
  /// Keep the disk snapshot for revert and dirty detection.
  const [disk, setDisk] = useCached<api.Advanced | null>("core.disk", null);
  const [draft, setDraft] = useCached<api.Advanced | null>("core.draft", null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.advancedGet().then(
      (options) => {
        setDisk(options);
        setDraft(options);
      },
      (e) => onMessage(failure(e)),
    );
  }, [onMessage]);

  const dirty = JSON.stringify(draft) !== JSON.stringify(disk);
  useEffect(() => {
    onUnsavedChange(dirty);
    return () => onUnsavedChange(false);
  }, [dirty, onUnsavedChange]);

  if (draft === null || disk === null) {
    return <Spinner label={t("Loading")} />;
  }

  const edit = (patch: Partial<api.Advanced>) => setDraft({ ...draft, ...patch });

  const save = async () => {
    setBusy(true);
    onMessage(null);
    try {
      // Show normalized values read back from disk.
      const written = await api.advancedSet(draft);
      setDisk(written);
      setDraft(written);
      onSaved();
      onMessage(notice(t("Saved. Changes take effect on the next connection.")));
    } catch (e) {
      onMessage(failure(e));
    } finally {
      setBusy(false);
    }
  };

  const groups: Group[] = [
    {
      id: "catch",
      label: t("Capture"),
      icon: Cable,
      parts: [
        {
          id: "catch-proxy",
          label: t("Local proxy"),
          hint: t("one port for HTTP and SOCKS5 — enter it in your browser in Proxy mode."),
          settings: [
            {
              id: "mixed-port",
              label: t("Port"),
              hint: t("changing this requires reconnection"),
              control: (
                <NumberInput
                  className="w-32"
                  value={draft.mixedPort}
                  disabled={busy}
                  min={1}
                  max={65535}
                  aria-label={t("Local proxy port")}
                  onChange={(value) => edit({ mixedPort: value ?? 0 })}
                />
              ),
            },
          ],
        },
        {
          id: "catch-tun",
          label: t("Virtual adapter"),
          hint: t("carries all device traffic. Enable TUN in Connection; configure it here."),
          settings: [
            {
              id: "stack",
              label: t("TUN stack"),
              control: (
                <ChoiceCards
                  aria-label={t("TUN stack")}
                  value={draft.stack}
                  options={cards(STACKS)}
                  disabled={busy}
                  onChange={(stack) => edit({ stack })}
                />
              ),
            },
            {
              id: "device",
              label: t("Adapter name"),
              hint: t("shown in Network Connections. Leave empty for the default name Meta"),
              control: (
                <Input
                  className="w-48"
                  value={draft.device}
                  disabled={busy}
                  placeholder="Meta"
                  aria-label={t("Adapter name")}
                  onChange={(event) => edit({ device: event.target.value })}
                />
              ),
            },
            {
              id: "mtu",
              label: "MTU",
              hint: t(
                "0 lets the core decide; otherwise 576–9000. Use the path MTU tool to measure",
              ),
              control: (
                // Empty means zero: let the core decide.
                <NumberInput
                  className="w-32"
                  allowEmpty
                  value={draft.mtu === 0 ? null : draft.mtu}
                  disabled={busy}
                  placeholder="0"
                  min={0}
                  max={9000}
                  aria-label="MTU"
                  onChange={(value) => edit({ mtu: value ?? 0 })}
                />
              ),
            },
            {
              id: "strict-route",
              label: "strict-route",
              inline: true,
              hint: t(
                "the core blocks routes outside the adapter. Similar purpose to kill-switch, different mechanism",
              ),
              control: (
                <Switch
                  aria-label="strict-route"
                  checked={draft.strictRoute}
                  disabled={busy}
                  onChange={(event) => edit({ strictRoute: event.target.checked })}
                />
              ),
            },
            {
              id: "dns-hijack",
              label: "dns-hijack",
              hint: (
                <>
                  {t(
                    "capture other DNS requests so apps with their own resolvers do not bypass the tunnel. Empty disables interception",
                  )}
                </>
              ),
              control: (
                <Textarea
                  mono
                  autoSize
                  value={lines(draft.dnsHijack)}
                  disabled={busy}
                  rows={2}
                  aria-label={t("DNS interception targets")}
                  onChange={(event) => edit({ dnsHijack: parse(event.target.value) })}
                />
              ),
            },
          ],
        },
      ],
    },
    {
      id: "names",
      label: t("Names"),
      icon: Globe,
      parts: [
        {
          id: "names-dns",
          label: t("Name resolution"),
          hint: t("the core resolves names without relying on system DNS."),
          settings: [
            {
              id: "dns-enable",
              label: t("Core DNS resolver"),
              inline: true,
              hint: t("always enabled in TUN mode regardless of this setting"),
              control: (
                <Switch
                  aria-label={t("Core DNS resolver")}
                  checked={draft.dnsEnable}
                  disabled={busy}
                  onChange={(event) => edit({ dnsEnable: event.target.checked })}
                />
              ),
            },
            {
              id: "enhanced-mode",
              label: "enhanced-mode",
              control: (
                <ChoiceCards
                  aria-label={t("Address resolution mode")}
                  value={draft.enhancedMode}
                  options={cards(ENHANCED)}
                  disabled={busy || !draft.dnsEnable}
                  onChange={(enhancedMode) => edit({ enhancedMode })}
                />
              ),
            },
            {
              id: "nameserver",
              label: "nameserver",
              hint: (
                <>
                  {t("one server per line")} — <Code>8.8.8.8</Code>, <Code>tls://1.1.1.1</Code>, DoH
                </>
              ),
              control: (
                <Textarea
                  mono
                  autoSize
                  value={lines(draft.nameserver)}
                  disabled={busy}
                  rows={3}
                  aria-label={t("Name servers")}
                  onChange={(event) => edit({ nameserver: parse(event.target.value) })}
                />
              ),
            },
          ],
        },
        {
          id: "names-sniff",
          label: t("Connection inspection"),
          hint: t("extracts the domain from connections made directly to an IP address."),
          settings: [
            {
              id: "sniffer",
              label: "Sniffing",
              inline: true,
              hint: t(
                "extract names from TLS and HTTP so domain rules can match externally resolved connections",
              ),
              control: (
                <Switch
                  aria-label="Sniffing"
                  checked={draft.sniffer}
                  disabled={busy}
                  onChange={(event) => edit({ sniffer: event.target.checked })}
                />
              ),
            },
          ],
        },
      ],
    },
    {
      id: "log",
      label: t("Logging"),
      icon: ScrollText,
      parts: [
        {
          id: "log-level",
          label: t("Verbosity"),
          hint: t("how much detail the core writes to Logs."),
          settings: [
            {
              id: "log",
              label: t("Level"),
              hint: t("debug is for troubleshooting and produces many lines"),
              control: (
                <Select
                  value={draft.logLevel}
                  disabled={busy}
                  aria-label={t("Log verbosity")}
                  onChange={(logLevel) => edit({ logLevel })}
                  options={LEVELS.map((level) => ({ value: level, label: level }))}
                />
              ),
            },
          ],
        },
      ],
    },
  ];

  return (
    <Page
      groups={groups}
      bar={
        <SectionBar
          start={start}
          hint={hint}
          end={
            <SaveActions dirty={dirty} busy={busy} onUndo={() => setDraft(disk)} onSave={save} />
          }
        />
      }
    />
  );
}
