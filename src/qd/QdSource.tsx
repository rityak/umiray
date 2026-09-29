import { Eye, EyeOff, Rss } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Button, Card, EmptyState, IconButton, Select, Text } from "rootik";
import { formatBytes } from "../api";
import { useCached } from "../hooks/useCached";
import { useNow } from "../hooks/useNow";
import { unchanged, usePoll } from "../hooks/usePoll";
import { t } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";
import Scroll from "../shell/Scroll";
import SectionBar from "../shell/SectionBar";
import LinkDialog from "../sources/LinkDialog";
import SourceCard from "../sources/SourceCard";
import * as qd from "./api";

type Props = {
  state: qd.State | null;
  hidden: boolean;
  onHidden: () => void;
  focus: boolean;
  onFocused: () => void;
  onChanged: () => void;
  onMessage: (message: Message) => void;
};

const EVERY = [30, 60, 180, 480, 1440];

function every(minutes: number): string {
  return minutes < 60 ? t("{n} min", { n: minutes }) : t("{n} h", { n: minutes / 60 });
}

function nextRefresh(last: number, minutes: number, now: number): string {
  if (minutes <= 0) return t("automatic refresh is off");
  const left = Math.round((last + minutes * 60_000 - now) / 60_000);
  if (last <= 0 || left <= 0) return t("next refresh is due now");
  if (left < 60) return t("next refresh in {n} min", { n: left });
  return t("next refresh in {hours} h {minutes} min", {
    hours: Math.floor(left / 60),
    minutes: left % 60,
  });
}

export default function QdSource({
  state,
  hidden,
  onHidden,
  focus,
  onFocused,
  onChanged,
  onMessage,
}: Props) {
  const [adding, setAdding] = useState(false);
  const [about, setAbout] = useCached<qd.About | null>("qd.about", null);
  const [settings, setSettings] = useCached<qd.Settings | null>("qd.settings", null);
  const [nodes, setNodes] = useCached<qd.Node[]>("qd.nodes", []);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const now = useNow(30_000);
  const imported = state?.imported ?? false;

  usePoll(() => {
    qd.about().then(unchanged(setAbout), () => {});
    qd.nodes().then(unchanged(setNodes), () => {});
  }, imported);
  usePoll(() => {
    qd.settings().then(unchanged(setSettings), () => {});
  }, Boolean(state));

  useEffect(() => {
    if (!focus) return;
    setAdding(true);
    onFocused();
  }, [focus, onFocused]);

  const take = useCallback(
    async (link: string) => {
      await qd.importLink(link);
      setAdding(false);
      onMessage(notice(t("qd link imported")));
      onChanged();
    },
    [onChanged, onMessage],
  );

  const refresh = useCallback(async () => {
    setBusy(true);
    try {
      await qd.refresh();
      onMessage(notice(t("Subscription refreshed")));
      onChanged();
    } catch (e) {
      onMessage(failure(e));
    } finally {
      setBusy(false);
    }
  }, [onChanged, onMessage]);

  const unlink = useCallback(async () => {
    try {
      await qd.unlink();
      onMessage(notice(t("qd link removed")));
      onChanged();
    } catch (e) {
      onMessage(failure(e));
    }
  }, [onChanged, onMessage]);

  const schedule = useCallback(
    async (minutes: number) => {
      try {
        setSettings(await qd.saveSettings({ refreshMinutes: minutes }));
      } catch (e) {
        onMessage(failure(e));
      }
    },
    [onMessage],
  );

  const shown = useMemo(() => nodes.map(qd.asNode), [nodes]);
  const minutes = settings?.refreshMinutes ?? state?.subscription.intervalMinutes ?? 480;
  const last = state?.subscription.lastRefresh ?? 0;
  const note = [
    about?.expiresAt
      ? t("until {date}", { date: new Date(about.expiresAt).toLocaleDateString() })
      : null,
    about ? `↓ ${formatBytes(about.down)} · ↑ ${formatBytes(about.up)}` : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <>
      {adding && (
        <LinkDialog
          onSubmit={take}
          onClose={() => setAdding(false)}
          copy={{
            description: t(
              "One qd:// link from your provider. It carries the entry nodes and the network key.",
            ),
            label: t("Link"),
            placeholder: "qd://…",
            type: "text",
          }}
        />
      )}
      <SectionBar
        end={
          <IconButton
            variant="ghost"
            icon={hidden ? <EyeOff /> : <Eye />}
            active={hidden}
            label={
              hidden
                ? t("Show addresses and subscription names")
                : t("Hide addresses and subscription names")
            }
            onClick={onHidden}
          />
        }
      />
      <div className="grid min-h-0 flex-1 grid-cols-[280px_minmax(0,1fr)] grid-rows-[minmax(0,1fr)] gap-3 max-[819px]:grid-cols-1">
        <Scroll>
          <Card title={imported ? t("Replace the qd link") : t("Add source")}>
            <div className="flex flex-col gap-1.5">
              <Button variant="primary" block icon={<Rss />} onClick={() => setAdding(true)}>
                {t("Link")}
              </Button>
              <Text tone="muted" size="xs" className="mt-1 block">
                {t("qd takes one link: a new one replaces the current subscription.")}
              </Text>
            </div>
          </Card>

          <Card title={t("Subscription refresh")} description={nextRefresh(last, minutes, now)}>
            <Select
              aria-label={t("Subscription refresh schedule")}
              value={String(minutes)}
              disabled={!settings}
              onChange={(value) => schedule(Number(value))}
              options={[...new Set([...EVERY, minutes])]
                .sort((a, b) => a - b)
                .map((value) => ({ value: String(value), label: every(value) }))}
            />
          </Card>
        </Scroll>

        <Scroll className="gap-2">
          {imported ? (
            <SourceCard
              source={{
                id: "qd",
                name: about?.tag || "qd",
                url: "qd://",
                updated: last > 0 ? Math.floor(last / 1000) : null,
                nodes: state?.nodes.total ?? nodes.length,
                records: false,
              }}
              note={note}
              hidden={hidden}
              nodes={shown}
              open={open}
              busy={busy}
              onToggle={() => setOpen((was) => !was)}
              onRefresh={refresh}
              onRemove={unlink}
              onDropNode={() => {}}
            />
          ) : (
            <Card>
              <EmptyState
                icon={<Rss />}
                title={t("No sources")}
                hint={t("Add the qd:// link from your provider.")}
              />
            </Card>
          )}
        </Scroll>
      </div>
    </>
  );
}
