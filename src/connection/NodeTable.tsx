import { Pencil } from "lucide-react";
import { Badge, DataTable, IconButton, useElementSize } from "rootik";
import * as api from "../api";
import type { NodeSpeed } from "../hooks/useTraffic";
import { t } from "../i18n";
import Flag from "../shell/Flag";
import { hide } from "../shell/secret";
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
        rows={nodes}
        rowKey={key}
        selectedKey={current ? key(current) : null}
        onRowClick={(node) => node.supported && onSelect(node.name)}
        // Right click edits, as on tiles (D-114); the pencil stays the keyboard path.
        rowProps={(node) => ({
          "data-node": node.name,
          onContextMenu: (event) => {
            if (!onEdit) return;
            event.preventDefault();
            onEdit(node.name);
          },
        })}
        columns={[
          {
            key: "name",
            header: t("Node"),
            sortable: true,
            value: (node) => node.name,
            cell: (node) => (
              <span className="inline-flex items-center gap-1.5">
                <Flag country={node.country} />
                {node.name}
              </span>
            ),
          },
          {
            key: "kind",
            header: t("Protocol"),
            sortable: true,
            value: (node) => node.kind,
            cell: (node) =>
              node.supported ? (
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
            cell: (node) => <NodeDelay node={node} />,
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
                  cell: (node: api.Node) => (
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
        ]}
      />
    </div>
  );
}
