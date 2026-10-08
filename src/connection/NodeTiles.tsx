import { memo } from "react";
import { Badge, Card, Progress, RollingNumber, StatusDot, Switch, Text } from "rootik";
import type { Node } from "../api";
import { targetLook } from "../config/kinds";
import type { NodeSpeed } from "../hooks/useTraffic";
import { t } from "../i18n";
import Flag from "../shell/Flag";
import { hide } from "../shell/secret";
import { isExit } from "./exits";
import { plainName, share } from "./groups";
import NodeDelay from "./NodeDelay";

/// Who is in AUTO while its make-up is edited (D-172): a switch on every tile.
export type AutoPick = {
  has: (node: Node) => boolean;
  /// The source is out as a whole: its nodes cannot be switched one by one.
  locked: (node: Node) => boolean;
  onToggle: (node: Node, on: boolean) => void;
};

type Props = {
  nodes: Node[];
  selected: string | null;
  /// Private mode (D-127): the address is covered with dots.
  hidden: boolean;
  rates: Record<string, NodeSpeed>;
  /// No transport details: no address, protocol or load (qd).
  plain?: boolean;
  onSelect: (node: string) => void;
  onEdit?: (node: string) => void;
  auto?: AutoPick;
};

/// Nodes as tiles: "pick a server with the mouse" (D-079, D-172).
export default function NodeTiles({
  nodes,
  selected,
  hidden,
  rates,
  plain,
  onSelect,
  onEdit,
  auto,
}: Props) {
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] content-start gap-3">
      {nodes.map((node) => (
        <Tile
          key={`${node.source}/${node.name}`}
          node={node}
          selected={selected === node.name}
          hidden={hidden}
          rate={rates[node.name]}
          load={share(node.name, rates)}
          plain={plain}
          onSelect={onSelect}
          // DIRECT and AUTO are the client's, not a source's: nothing to edit (D-166).
          onEdit={isExit(node) ? undefined : onEdit}
          inAuto={auto?.has(node)}
          locked={auto?.locked(node)}
          onAuto={auto?.onToggle}
        />
      ))}
    </div>
  );
}

type TileProps = {
  node: Node;
  selected: boolean;
  hidden: boolean;
  rate: NodeSpeed | undefined;
  /// Share of the busiest node's connections, 0…1: the bar under the tile.
  load: number;
  plain?: boolean;
  onSelect: (node: string) => void;
  onEdit?: (node: string) => void;
  inAuto?: boolean;
  locked?: boolean;
  onAuto?: (node: Node, on: boolean) => void;
};

/// One tile. Separate and under `memo`: rates arrive every poll tick but change for one or
/// two nodes — no reason to redraw the other hundreds of tiles.
const Tile = memo(function Tile({
  node,
  selected,
  hidden,
  rate,
  load,
  plain,
  onSelect,
  onEdit,
  inAuto,
  locked,
  onAuto,
}: TileProps) {
  // Значок выхода — тот же, что у цели правила в «Маршрутизации»: одно имя, один облик.
  const look = isExit(node) ? targetLook(node.name, []) : null;
  // A node the core will not bring up cannot be selected (D-063).
  const pick = node.supported && !onAuto ? () => onSelect(node.name) : undefined;
  const connections = rate?.connections ?? 0;
  return (
    <Card
      data-node={node.name}
      variant="outline"
      padding="sm"
      selected={selected}
      // While AUTO's make-up is edited the tile holds a switch and picks nothing.
      onAction={pick}
      title={
        <span className="flex min-w-0 items-center gap-2">
          {look ? <look.Icon className="size-4 shrink-0" /> : <Flag country={node.country} />}
          <span className="truncate">{plainName(node.name)}</span>
        </span>
      }
      actions={
        <span className="flex items-center gap-1.5">
          {/* До DIRECT и AUTO мерить нечего: прочерк стоял бы там всегда. */}
          {!isExit(node) && <NodeDelay node={node} />}
          {onAuto && inAuto !== undefined && (
            <Switch
              size="sm"
              aria-label={t("{name} in AUTO", { name: node.name })}
              checked={inAuto}
              disabled={locked || !node.supported}
              onChange={(event) => onAuto(node, event.target.checked)}
            />
          )}
        </span>
      }
      dense
      className="select-none"
      onContextMenu={
        onEdit &&
        ((event) => {
          event.preventDefault();
          onEdit(node.name);
        })
      }
    >
      {!plain && (
        <div className="flex flex-col gap-2">
          <Text tone="muted" size="xs" truncate>
            {node.address === null ? node.kind : hide(node.address, hidden)}
          </Text>
          {!isExit(node) && (
            // One line, one centre line: the badges and the count share it.
            <div className="flex items-center justify-between gap-2">
              <span className="flex min-w-0 flex-wrap items-center gap-1">
                <Badge size="sm" variant="outline">
                  {node.kind}
                </Badge>
                {!node.supported && (
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
              <span className="flex shrink-0 items-center gap-1.5">
                <StatusDot
                  tone={connections > 0 ? "success" : "neutral"}
                  pulse={connections > 0}
                  label={connections > 0 ? t("Traffic") : t("Idle")}
                  hideLabel
                />
                <Text size="xs">
                  <RollingNumber value={connections} />{" "}
                  <Text tone="muted" size="xs">
                    {t("conn.")}
                  </Text>
                </Text>
              </span>
            </div>
          )}
        </div>
      )}
      {load > 0 && (
        // The node's share of the load: under the tile, the wider the busier (D-172).
        <Progress
          variant="edge"
          tone="accent"
          value={Math.round(load * 100)}
          aria-label={t("Load")}
        />
      )}
    </Card>
  );
});
