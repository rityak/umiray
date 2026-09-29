import { Eye, EyeOff, Gauge, LayoutGrid, RefreshCw, Server, Table2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Badge,
  Button,
  Callout,
  Card,
  EmptyState,
  IconButton,
  SegmentedControl,
  Select,
  Tooltip,
} from "rootik";
import * as api from "../api";
import type { NodeSpeed } from "../hooks/useTraffic";
import { locale, t } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";
import { hide } from "../shell/secret";
import NodeEditor from "./NodeEditor";
import NodeTable from "./NodeTable";
import NodeTiles from "./NodeTiles";

type Props = {
  nodes: api.Node[];
  selected: string | null;
  /// RULES assigns routes through rules; the banner offers switching to MANUAL (D-056).
  rules: boolean;
  /// Sources name the nodes and give the list its default order. None — no such order.
  sources: api.Source[];
  /// Privacy mode hides server addresses in this section (D-127).
  hidden: boolean;
  /// App polls live traffic regardless of the open section (S-018).
  rates: Record<string, NodeSpeed>;
  onSelect: (node: string) => void;
  onManual: () => void;
  onChanged: () => void;
  onAdd: () => void;
  onMessage: (message: Message) => void;
  /// Fetch the list again where it comes from. The list says what it refreshes.
  onRefresh: () => Promise<void>;
  refreshLabel: string;
  emptyHint: string;
  // What the engine can do with its nodes (D-154): absent means the list does not offer it.
  /// Latency measured on request; the method names the column (D-069).
  measure?: { method: api.PingMethod; run: () => Promise<void> };
  /// The list shows addresses, so it carries the privacy toggle (D-127).
  onHidden?: () => void;
  /// Nodes of our own sources can be edited (D-114).
  editable?: boolean;
  /// No transport details: no address, protocol, source or per-node load.
  plain?: boolean;
};

type Sort = "source" | "name" | "delay";
/// Session-only view choice, like form/code in other sections (D-079).
type View = "tiles" | "table";

/// Unmeasured nodes sort last, rather than appearing fastest.
function byDelay(a: api.Node, b: api.Node): number {
  const left = a.delay ?? Number.POSITIVE_INFINITY;
  const right = b.delay ?? Number.POSITIVE_INFINITY;
  return left - right;
}

/// Compare nodes across all sources.
export default function Nodes({
  nodes,
  selected,
  rules,
  sources,
  hidden,
  rates,
  onSelect,
  onManual,
  onChanged,
  onAdd,
  onMessage,
  onRefresh,
  refreshLabel,
  emptyHint,
  measure: measurer,
  onHidden,
  editable,
  plain,
}: Props) {
  const bySource = sources.length > 0;
  const [picked, setSort] = useState<Sort>("source");
  /// No sources — no source order: the list falls back to latency. Computed, not stored:
  /// the sources arrive after the first render.
  const sort = bySource || picked !== "source" ? picked : "delay";
  const [view, setView] = useState<View>("tiles");
  const [measuring, setMeasuring] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [rulesHintHidden, setRulesHintHidden] = useState(() => {
    try {
      return localStorage.getItem("umiray:rules-hint-hidden") === "true";
    } catch {
      return false;
    }
  });
  /// Right click edits; left click still selects (D-114).
  const [editing, setEditing] = useState<string | null>(null);

  /// Keep tile callbacks stable until sources or privacy mode change.
  const sourceName = useCallback(
    (id: string) => hide(sources.find((item) => item.id === id)?.name ?? id, hidden) ?? id,
    [sources, hidden],
  );

  const visible = useMemo(() => {
    const sorted = [...nodes];
    if (sort === "name") sorted.sort((a, b) => a.name.localeCompare(b.name, locale()));
    if (sort === "delay") sorted.sort(byDelay);
    return sorted;
  }, [nodes, sort]);

  /// User-triggered checks report failures; automatic checks stay silent so proxy
  /// measurements do not show a connect-first banner on every visit (D-062, D-069).
  const measure = useRef(async (_report: boolean) => {});
  measure.current = async (report: boolean) => {
    if (!measurer) return;
    setMeasuring(true);
    try {
      await measurer.run();
      onChanged();
    } catch (e) {
      if (report) onMessage(failure(e));
    } finally {
      setMeasuring(false);
    }
  };

  /// Refresh the list where it comes from, then measure latency again.
  const refresh = async () => {
    setRefreshing(true);
    try {
      await onRefresh();
      onChanged();
    } catch (e) {
      onMessage(failure(e));
    } finally {
      setRefreshing(false);
    }
    await measure.current(false);
  };

  /// Measure an unchecked list once, then only on request to avoid repeated pings.
  const measured = useRef(false);
  useEffect(() => {
    if (!measurer || measured.current || nodes.length === 0) return;
    if (nodes.some((node) => node.delay !== null)) {
      measured.current = true;
      return;
    }
    measured.current = true;
    measure.current(false);
  }, [nodes, measurer]);

  const select = useCallback(
    (node: string) => {
      if (rules) {
        onMessage(notice(t("Switch to Manual first — the route has not changed.")));
        return;
      }
      onSelect(node);
    },
    [rules, onMessage, onSelect],
  );

  if (nodes.length === 0) {
    return (
      <Card className="h-full min-h-0 justify-center">
        <EmptyState
          icon={<Server />}
          title={t("No nodes")}
          hint={emptyHint}
          action={
            <Button variant="primary" onClick={onAdd}>
              {t("Add source")}
            </Button>
          }
        />
      </Card>
    );
  }

  return (
    <Card
      padding="sm"
      className="h-full min-h-0"
      title={
        <span className="inline-flex items-center gap-2">
          {t("Nodes")} <Badge size="sm">{nodes.length}</Badge>
        </span>
      }
    >
      {/* Only the list below the toolbar scrolls. */}
      <div className="flex h-full min-h-0 flex-col gap-2">
        <div className="flex shrink-0 flex-wrap items-center gap-1.5">
          <Select<Sort>
            size="sm"
            className="w-44"
            aria-label={t("Node order")}
            value={sort}
            onChange={setSort}
            options={[
              ...(bySource ? [{ value: "source" as const, label: t("by source") }] : []),
              { value: "name", label: t("by name") },
              { value: "delay", label: t("by latency") },
            ]}
          />
          <SegmentedControl<View>
            size="sm"
            aria-label={t("List view")}
            value={view}
            onChange={setView}
            options={[
              { value: "tiles", icon: <LayoutGrid />, hint: t("Tiles") },
              { value: "table", icon: <Table2 />, hint: t("Table") },
            ]}
          />
          {onHidden && (
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
          )}
          <IconButton
            size="sm"
            variant="ghost"
            icon={<RefreshCw />}
            loading={refreshing}
            disabled={measuring}
            label={refreshLabel}
            onClick={refresh}
          />
          <span className="flex-1" />
          {measurer && (
            <Tooltip
              content={t("Latency to each server: {method}. Change the method in Settings", {
                method: api.PING_LABEL[measurer.method],
              })}
            >
              <Button
                size="sm"
                icon={<Gauge />}
                loading={measuring}
                disabled={refreshing}
                onClick={() => measure.current(true)}
              >
                {t("Check latency")}
              </Button>
            </Tooltip>
          )}
        </div>
        {rules && !rulesHintHidden && (
          <Callout
            tone="info"
            onDismiss={() => {
              setRulesHintHidden(true);
              try {
                localStorage.setItem("umiray:rules-hint-hidden", "true");
              } catch {
                // Still dismiss for this session when storage is unavailable.
              }
            }}
            actions={
              <Button size="sm" onClick={onManual}>
                {t("Choose manually")}
              </Button>
            }
          >
            {t("In Rules mode, your rules assign the exits.")}
          </Callout>
        )}
        {/* The table owns its scroll so its header stays sticky. */}
        <div
          className={
            view === "tiles" ? "-mx-1 min-h-0 flex-1 overflow-y-auto px-1 pb-1" : "min-h-0 flex-1"
          }
        >
          {view === "tiles" ? (
            <NodeTiles
              nodes={visible}
              selected={selected}
              sourceName={sourceName}
              hidden={hidden}
              rates={rates}
              plain={plain}
              onSelect={select}
              onEdit={editable ? setEditing : undefined}
            />
          ) : (
            <NodeTable
              nodes={visible}
              selected={selected}
              method={measurer?.method}
              sourceName={sourceName}
              hidden={hidden}
              rates={rates}
              plain={plain}
              onSelect={select}
              onEdit={editable ? setEditing : undefined}
            />
          )}
        </div>
      </div>

      {editing !== null &&
        (() => {
          // Resolve from the latest list so saved edits are reflected immediately.
          const node = nodes.find((item) => item.name === editing);
          return node === undefined ? null : (
            <NodeEditor
              node={node}
              onClose={() => setEditing(null)}
              onChanged={onChanged}
              onMessage={onMessage}
            />
          );
        })()}
    </Card>
  );
}
