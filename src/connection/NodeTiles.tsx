import { Pencil } from "lucide-react";
import { memo } from "react";
import { Badge, IconButton, Item } from "rootik";
import type { Node } from "../api";
import { targetLook } from "../config/kinds";
import type { NodeSpeed } from "../hooks/useTraffic";
import { t } from "../i18n";
import Flag from "../shell/Flag";
import { hide } from "../shell/secret";
import { isExit } from "./exits";
import NodeDelay from "./NodeDelay";
import { nowText } from "./rate";

type Props = {
  nodes: Node[];
  selected: string | null;
  sourceName: (id: string) => string;
  /// Private mode (D-127): the address is covered with dots.
  hidden: boolean;
  rates: Record<string, NodeSpeed>;
  plain?: boolean;
  onSelect: (node: string) => void;
  onEdit?: (node: string) => void;
};

/// Nodes as tiles: "pick a server with the mouse" (D-079). Comparing by column is the table's job.
export default function NodeTiles({
  nodes,
  selected,
  sourceName,
  hidden,
  rates,
  plain,
  onSelect,
  onEdit,
}: Props) {
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] content-start gap-2">
      {nodes.map((node) => (
        <Tile
          key={`${node.source}/${node.name}`}
          node={node}
          selected={selected === node.name}
          source={sourceName(node.source)}
          hidden={hidden}
          rate={rates[node.name]}
          plain={plain}
          onSelect={onSelect}
          // DIRECT and AUTO are the client's, not a source's: nothing to edit (D-166).
          onEdit={isExit(node) ? undefined : onEdit}
        />
      ))}
    </div>
  );
}

type TileProps = {
  node: Node;
  selected: boolean;
  source: string;
  hidden: boolean;
  rate: NodeSpeed | undefined;
  plain?: boolean;
  onSelect: (node: string) => void;
  onEdit?: (node: string) => void;
};

/// One tile. Separate and under `memo`: rates arrive every poll tick but change for one or
/// two nodes — no reason to redraw the other hundreds of tiles.
const Tile = memo(function Tile({
  node,
  selected,
  source,
  hidden,
  rate,
  plain,
  onSelect,
  onEdit,
}: TileProps) {
  // Значок выхода — тот же, что у цели правила в «Маршрутизации»: одно имя, один облик.
  const look = isExit(node) ? targetLook(node.name, []) : null;
  return (
    <Item
      data-node={node.name}
      variant="surface"
      selected={selected}
      // A node the core will not bring up cannot be selected (D-063).
      disabled={!node.supported}
      onClick={() => onSelect(node.name)}
      onContextMenu={
        onEdit &&
        ((event) => {
          event.preventDefault();
          onEdit(node.name);
        })
      }
      icon={look && !plain ? <look.Icon /> : undefined}
      iconTone={look?.tone}
      media={plain || look ? undefined : <Flag country={node.country} />}
      title={node.name}
      // Three lines, as before (D-085): who in the title; how and where; what now and from where.
      description={
        !plain && (
          <>
            <span className="block truncate">
              {node.address === null ? node.kind : `${hide(node.address, hidden)} · ${node.kind}`}
            </span>
            <span className="block truncate">
              {rate && `${nowText(rate)} · ${t("{n} conn.", { n: rate.connections })} · `}
              {source}
            </span>
          </>
        )
      }
      meta={
        <span className="flex flex-col items-end gap-1">
          {/* До DIRECT и AUTO мерить нечего: прочерк стоял бы там всегда. */}
          {!isExit(node) && <NodeDelay node={node} />}
          {!node.supported && !isExit(node) && (
            <Badge size="sm" tone="danger">
              {t("not supported")}
            </Badge>
          )}
          {node.edited && (
            <Badge size="sm" variant="outline">
              {t("edited")}
            </Badge>
          )}
        </span>
      }
      actions={
        onEdit && (
          <IconButton
            size="sm"
            variant="ghost"
            icon={<Pencil />}
            label={t("Edit {name}", { name: node.name })}
            onClick={() => onEdit(node.name)}
          />
        )
      }
    />
  );
});
