import { Pencil } from "lucide-react";
import { useState } from "react";
import {
  Badge,
  type Column,
  DataTable,
  IconButton,
  type SortDir,
  type SortState,
  useElementSize,
} from "rootik";
import * as api from "../api";
import { targetLook } from "../config/kinds";
import type { NodeSpeed } from "../hooks/useTraffic";
import { locale, t } from "../i18n";
import Flag from "../shell/Flag";
import { hide } from "../shell/secret";
import { isExit } from "./exits";
import NodeDelay from "./NodeDelay";
import { nowText } from "./rate";

type Props = {
  nodes: api.Node[];
  selected: string | null;
  method?: api.PingMethod;
  sourceName: (id: string) => string;
  hidden: boolean;
  rates: Record<string, NodeSpeed>;
  plain?: boolean;
  onSelect: (node: string) => void;
  onEdit?: (node: string) => void;
};

const key = (node: api.Node) => `${node.source}/${node.name}`;

/// Below this width the columns do not fit and the table slides sideways. Address and source
/// go: both are visible in the tiles, the address also in the node editor.
const WIDE = 720;

/// Column order for nodes: numbers by value, text by the window's language.
function byColumn(value: NonNullable<Column<api.Node>["value"]>, dir: SortDir) {
  const sign = dir === "asc" ? 1 : -1;
  return (a: api.Node, b: api.Node) => {
    const x = value(a);
    const y = value(b);
    const order =
      typeof x === "number" && typeof y === "number"
        ? Math.sign(x - y) || 0
        : String(x ?? "").localeCompare(String(y ?? ""), locale());
    return sign * order;
  };
}

/// Nodes as a table: here people compare by column rather than pick by eye (D-079).
export default function NodeTable({
  nodes,
  selected,
  method,
  sourceName,
  hidden,
  rates,
  plain,
  onSelect,
  onEdit,
}: Props) {
  const rate = (node: api.Node) => {
    const speed = rates[node.name];
    return speed === undefined ? -1 : speed.down + speed.up;
  };
  const current = nodes.find((node) => node.name === selected);
  const box = useElementSize<HTMLDivElement>();
  /// The table sorts through us: DIRECT and AUTO stay on top under any column (D-166),
  /// and the kit has no pinned rows.
  const [sort, setSort] = useState<SortState | null>(null);
  const columns: Column<api.Node>[] = [
    {
      key: "name",
      header: t("Node"),
      sortable: true,
      value: (node) => node.name,
      cell: (node) => {
        const Icon = isExit(node) ? targetLook(node.name, []).Icon : null;
        return (
          <span className="inline-flex items-center gap-1.5">
            {Icon ? <Icon size={14} aria-hidden /> : <Flag country={node.country} />}
            {node.name}
          </span>
        );
      },
    },
    {
      key: "kind",
      header: t("Protocol"),
      sortable: true,
      value: (node) => node.kind,
      cell: (node) =>
        node.supported || isExit(node) ? (
          node.kind
        ) : (
          <Badge size="sm" tone="danger">
            {node.kind} · {t("not supported")}
          </Badge>
        ),
    },
    {
      key: "address",
      header: t("Address"),
      mono: true,
      value: (node) => hide(node.address, hidden) ?? "—",
    },
    {
      key: "delay",
      header: method ? t("Delay · {method}", { method: api.PING_LABEL[method] }) : t("Delay"),
      sortable: true,
      align: "end",
      value: (node) => node.delay ?? Number.POSITIVE_INFINITY,
      cell: (node) => !isExit(node) && <NodeDelay node={node} />,
    },
    {
      key: "source",
      header: t("Source"),
      sortable: true,
      value: (node) => sourceName(node.source),
      cell: (node) => (
        <>
          {sourceName(node.source)}
          {node.edited && (
            <Badge size="sm" variant="outline" className="ml-1.5">
              {t("edited")}
            </Badge>
          )}
        </>
      ),
    },
    {
      key: "now",
      header: t("Now"),
      sortable: true,
      align: "end",
      value: rate,
      cell: (node) => nowText(rates[node.name]),
    },
    ...(onEdit
      ? [
          {
            key: "edit",
            header: "",
            width: 44,
            cell: (node: api.Node) =>
              !isExit(node) && (
                <IconButton
                  size="sm"
                  variant="ghost"
                  icon={<Pencil />}
                  label={t("Edit {name}", { name: node.name })}
                  onClick={(event) => {
                    event.stopPropagation();
                    onEdit(node.name);
                  }}
                />
              ),
          },
        ]
      : []),
  ];
  const by = sort && columns.find((column) => column.key === sort.key)?.value;
  const rows =
    sort && by
      ? [
          ...nodes.filter(isExit),
          ...nodes.filter((node) => !isExit(node)).sort(byColumn(by, sort.dir)),
        ]
      : nodes;
  return (
    // `relative overflow-hidden`: the kit's cells have `sr-only` with `position: absolute`,
    // and the table wrapper is not positioned — without this they attach to the card, escape
    // the table's scroll and stretch the page (GOTCHAS).
    <div ref={box.ref} className="relative h-full overflow-hidden">
      <DataTable
        sticky
        maxHeight="100%"
        hiddenColumns={
          plain
            ? ["kind", "address", "source", "now"]
            : box.width > 0 && box.width < WIDE
              ? ["address", "source"]
              : undefined
        }
        density="compact"
        rows={rows}
        sort={sort}
        onSortChange={setSort}
        rowKey={key}
        selectedKey={current ? key(current) : null}
        onRowClick={(node) => node.supported && onSelect(node.name)}
        // Right click edits, as on tiles (D-114); the pencil stays the keyboard path.
        rowProps={(node) => ({
          "data-node": node.name,
          onContextMenu: (event) => {
            if (!onEdit || isExit(node)) return;
            event.preventDefault();
            onEdit(node.name);
          },
        })}
        columns={columns}
      />
    </div>
  );
}
