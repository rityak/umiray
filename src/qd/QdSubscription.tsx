import { Eye, EyeOff, Plus, RefreshCw, Rss, Trash2 } from "lucide-react";
import { useCallback, useState } from "react";
import { Badge, Button, Card, ConfirmButton, EmptyState, IconButton, Select, Text } from "rootik";
import { formatBytes } from "../api";
import { useCached } from "../hooks/useCached";
import { useNow } from "../hooks/useNow";
import { unchanged, usePoll } from "../hooks/usePoll";
import { locale, t } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";
import { hide } from "../shell/secret";
import AddMenu, { type AddKind } from "../sources/AddMenu";
import * as qd from "./api";

type Props = {
  state: qd.State | null;
  hidden: boolean;
  onHidden: () => void;
  onChanged: () => void;
  onAdd: (kind: AddKind) => void;
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

/// Подписка qd — карточка над его узлами в «Соединении». У qd она одна, и узлы видны рядом,
/// поэтому переключателя «Узлы · Источники» у qd нет: новая ссылка заменяет текущую.
export default function QdSubscription({
  state,
  hidden,
  onHidden,
  onChanged,
  onAdd,
  onMessage,
}: Props) {
  const [about, setAbout] = useCached<qd.About | null>("qd.about", null);
  const [settings, setSettings] = useCached<qd.Settings | null>("qd.settings", null);
  const [busy, setBusy] = useState(false);
  const now = useNow(30_000);
  const imported = state?.imported ?? false;

  usePoll(() => {
    qd.about().then(unchanged(setAbout), () => {});
  }, imported);
  usePoll(() => {
    qd.settings().then(unchanged(setSettings), () => {});
  }, Boolean(state));

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

  const add = (
    <AddMenu
      onPick={onAdd}
      trigger={<IconButton size="sm" variant="ghost" icon={<Plus />} label={t("Add source")} />}
    />
  );

  if (!imported) {
    return (
      <Card padding="sm" title={t("Subscription")} actions={add}>
        <EmptyState
          icon={<Rss />}
          title={t("No sources")}
          hint={t("Add the qd:// link from your provider. A new link replaces the current one.")}
          action={
            <AddMenu
              onPick={onAdd}
              trigger={<Button variant="primary">{t("Add source")}</Button>}
            />
          }
        />
      </Card>
    );
  }

  const minutes = settings?.refreshMinutes ?? state?.subscription.intervalMinutes ?? 480;
  const last = state?.subscription.lastRefresh ?? 0;
  const note = [
    last > 0 ? new Date(last).toLocaleString(locale()) : t("never refreshed"),
    about?.expiresAt
      ? t("until {date}", { date: new Date(about.expiresAt).toLocaleDateString() })
      : null,
    about ? `↓ ${formatBytes(about.down)} · ↑ ${formatBytes(about.up)}` : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <Card
      padding="sm"
      title={
        <span className="inline-flex items-center gap-2">
          {hide(about?.tag || "qd", hidden)}
          <Badge size="sm" variant="outline">
            {t("subscription")}
          </Badge>
        </span>
      }
      description={note}
      actions={
        <>
          <Button size="sm" icon={<RefreshCw />} loading={busy} onClick={refresh}>
            {t("Refresh")}
          </Button>
          <IconButton
            size="sm"
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
          {add}
          <ConfirmButton
            size="sm"
            variant="ghost"
            icon={<Trash2 />}
            confirmLabel={t("Delete for sure?")}
            onConfirm={unlink}
          >
            {t("Delete")}
          </ConfirmButton>
        </>
      }
    >
      <div className="flex flex-wrap items-center gap-1.5">
        <Text tone="muted" size="xs">
          {t("Auto-refresh")}
        </Text>
        <Select
          size="sm"
          className="w-32"
          aria-label={t("Subscription refresh schedule")}
          value={String(minutes)}
          disabled={!settings}
          onChange={(value) => schedule(Number(value))}
          options={[...new Set([...EVERY, minutes])]
            .sort((a, b) => a - b)
            .map((value) => ({ value: String(value), label: every(value) }))}
        />
        <Text tone="muted" size="xs">
          {nextRefresh(last, minutes, now)}
        </Text>
      </div>
    </Card>
  );
}
