import { ExternalLink, ListChecks, Plus, RefreshCw, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import {
  Badge,
  Button,
  Card,
  EmptyState,
  IconButton,
  Item,
  ItemGroup,
  Text,
  Tooltip,
} from "rootik";
import * as api from "../api";
import { useCached } from "../hooks/useCached";
import { getLang, locale, t, tn } from "../i18n";
import { failure, type Message } from "../shell/Banner";
import { githubPageOf } from "./github";
import ListDialog from "./ListDialog";
import PriorityPicker from "./PriorityPicker";
import TargetPicker from "./TargetPicker";

type Props = {
  /// The route's `rule-sets` from the draft (D-158): the order is the priority.
  entries: api.RuleSetUse[];
  onChange: (entries: api.RuleSetUse[]) => void;
  /// "Where to" — the same choices a rule has.
  targets: string[];
  nodes: string[];
  onMessage: (message: Message | null) => void;
};

/// A list older than this is worth saying out loud: the registry changes daily (S-028).
const STALE_DAYS = 30;

function date(seconds: number): string {
  return new Date(seconds * 1000).toLocaleDateString(locale());
}

/// What the list holds and when it was fetched — one line under its name.
function summary(list: api.RuleList): string {
  const parts: string[] = [];
  // The form is chosen by the number, the text shows it with digit groups: 982 094, not 982094.
  const grouped = (n: number) => ({ n: n.toLocaleString(locale()) });
  if (list.domains > 0)
    parts.push(tn(list.domains, "{n} domain", "{n} domains", grouped(list.domains)));
  if (list.cidrs > 0) parts.push(tn(list.cidrs, "{n} subnet", "{n} subnets", grouped(list.cidrs)));
  if (list.updated) parts.push(t("fetched {date}", { date: date(list.updated) }));
  return parts.join(" · ");
}

function staleDays(list: api.RuleList): number | null {
  if (!list.published) return null;
  const days = Math.floor((Date.now() / 1000 - list.published) / 86_400);
  return days >= STALE_DAYS ? days : null;
}

/**
 * Rule sets of the route (D-157, D-158): downloaded lists of domains and subnets, each with
 * its own exit. They work in every direction — an exit through a proxy is used even in
 * Direct. The client downloads a list when it is added; the core builds its own format.
 */
export default function RuleSets({ entries, onChange, targets, nodes, onMessage }: Props) {
  const [cache, setCache] = useCached<api.RuleList[]>("lists", []);
  const [offers, setOffers] = useCached<api.ListOffer[]>("lists.catalog", []);
  const [adding, setAdding] = useState(false);
  /// What is being refreshed: an id, or `*` for all.
  const [busy, setBusy] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setCache(await api.listsList());
    } catch (e) {
      onMessage(failure(e));
    }
  }, [onMessage]);

  useEffect(() => {
    reload();
    api.listsCatalog().then(setOffers, () => setOffers([]));
  }, [reload]);

  const title = (id: string): string => {
    const known = cache.find((list) => list.id === id) ?? offers.find((offer) => offer.id === id);
    if (!known) return id;
    return getLang() === "en" ? (known.titleEn ?? known.title) : known.title;
  };

  /// Where the list lives: downloaded — its own addresses, not yet — the catalog's or the one
  /// it was added by.
  const page = (entry: api.RuleSetUse): string | null =>
    githubPageOf(
      cache.find((list) => list.id === entry.id)?.urls ??
        offers.find((offer) => offer.id === entry.id)?.urls ??
        (entry.url ? [entry.url] : []),
    );

  const browse = async (url: string) => {
    try {
      await api.systemOpenGithub(url);
    } catch (e) {
      onMessage(failure(e));
    }
  };

  const refresh = async (entry?: api.RuleSetUse) => {
    setBusy(entry?.id ?? "*");
    try {
      if (!entry) await api.listsRefresh();
      else if (cache.some((list) => list.id === entry.id)) await api.listsRefresh(entry.id);
      else await api.listsFetch(entry.id, entry.url);
      onMessage(null);
    } catch (e) {
      onMessage(failure(e));
    } finally {
      setBusy(null);
      await reload();
    }
  };

  const added = async (list: api.RuleList, url?: string) => {
    setAdding(false);
    onChange([...entries, { id: list.id, url, target: "umiray" }]);
    await reload();
    onMessage({
      tone: "info",
      text: t("«{name}» added to the route — save to apply.", { name: title(list.id) }),
      details: [],
    });
  };

  return (
    <Card
      headingLevel={3}
      icon={<ListChecks />}
      title="Rule sets"
      description={t("downloaded lists of domains and subnets")}
      actions={
        <>
          {cache.length > 0 && (
            <Button
              size="sm"
              variant="ghost"
              icon={<RefreshCw />}
              loading={busy === "*"}
              disabled={busy !== null}
              onClick={() => refresh()}
            >
              {t("Refresh all")}
            </Button>
          )}
          <Button size="sm" icon={<Plus />} onClick={() => setAdding(true)}>
            {t("Add")}
          </Button>
        </>
      }
    >
      {entries.length === 0 ? (
        <EmptyState
          size="sm"
          icon={<ListChecks />}
          title={t("No rule sets in this route")}
          hint={t("antizapret, antifilter, Telegram and more — from the catalog or by address.")}
          action={
            <Button icon={<Plus />} onClick={() => setAdding(true)}>
              {t("Add")}
            </Button>
          }
        />
      ) : (
        <ItemGroup variant="divided">
          {entries.map((entry, at) => {
            const list = cache.find((item) => item.id === entry.id);
            const stale = list ? staleDays(list) : null;
            const github = page(entry);
            return (
              <Item
                key={entry.id}
                size="sm"
                title={
                  <span className="inline-flex items-center gap-2">
                    {title(entry.id)}
                    <Text tone="muted" size="xs" className="rk-mono">
                      {entry.id}
                    </Text>
                  </span>
                }
                description={
                  list ? (list.error ?? summary(list)) : t("not downloaded yet — press refresh")
                }
                meta={
                  !list ? (
                    <Badge size="sm" tone="warn">
                      {t("not downloaded")}
                    </Badge>
                  ) : list.error ? (
                    <Badge size="sm" tone="danger">
                      {t("not refreshed")}
                    </Badge>
                  ) : stale !== null ? (
                    <Tooltip
                      content={t("The source itself was last updated {date}.", {
                        date: date(list.published ?? 0),
                      })}
                    >
                      <Badge size="sm" tone="warn">
                        {tn(stale, "{n} day old", "{n} days old")}
                      </Badge>
                    </Tooltip>
                  ) : null
                }
                actions={
                  <>
                    <div className="w-[130px]">
                      <PriorityPicker
                        value={entry.priority}
                        label={t("Priority of «{name}»", { name: title(entry.id) })}
                        onChange={(priority) =>
                          onChange(
                            entries.map((item, i) => (i === at ? { ...item, priority } : item)),
                          )
                        }
                      />
                    </div>
                    <div className="w-[200px]">
                      <TargetPicker
                        value={entry.target}
                        groups={targets}
                        nodes={nodes}
                        label={t("Where to send «{name}»", { name: title(entry.id) })}
                        onChange={(target) =>
                          onChange(
                            entries.map((item, i) => (i === at ? { ...item, target } : item)),
                          )
                        }
                      />
                    </div>
                    {github && (
                      <IconButton
                        size="sm"
                        variant="ghost"
                        icon={<ExternalLink />}
                        label={t("Open «{name}» on GitHub", { name: title(entry.id) })}
                        onClick={() => browse(github)}
                      />
                    )}
                    <IconButton
                      size="sm"
                      variant="ghost"
                      icon={<RefreshCw />}
                      label={t("Refresh «{name}»", { name: title(entry.id) })}
                      loading={busy === entry.id}
                      disabled={busy !== null}
                      onClick={() => refresh(entry)}
                    />
                    <IconButton
                      size="sm"
                      variant="ghost"
                      icon={<X />}
                      label={t("Remove «{name}» from the route", { name: title(entry.id) })}
                      onClick={() => onChange(entries.filter((_, i) => i !== at))}
                    />
                  </>
                }
              />
            );
          })}
        </ItemGroup>
      )}

      {adding && (
        <ListDialog
          taken={entries.map((entry) => entry.id)}
          onClose={() => setAdding(false)}
          onCatalog={async (id) => added(await api.listsFetch(id))}
          onUrl={async (url, name) => added(await api.listsAddUrl(url, name), url)}
        />
      )}
    </Card>
  );
}
