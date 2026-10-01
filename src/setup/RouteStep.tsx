import { useEffect, useState } from "react";
import { Spinner, Text } from "rootik";
import * as api from "../api";
import { choice, chosen, exits } from "../connection/exits";
import NodeTiles from "../connection/NodeTiles";
import { t } from "../i18n";

type Props = {
  direction: api.Direction;
  node: string | null;
  onChange: (direction: api.Direction, node: string | null) => void;
};

/// Шаг мастера «Маршрут» (D-162): тот же список, что в «Соединении», — DIRECT, AUTO
/// и узлы (D-166).
export default function RouteStep({ direction, node, onChange }: Props) {
  const [nodes, setNodes] = useState<api.Node[] | null>(null);

  useEffect(() => {
    api.nodesList().then(
      (list) => setNodes(list.filter((item) => item.supported)),
      () => setNodes([]),
    );
  }, []);

  if (nodes === null) return <Spinner label={t("Reading nodes…")} />;
  return (
    <div className="flex min-h-0 flex-col gap-3">
      <div className="-mx-1 max-h-72 overflow-y-auto px-1 pb-1">
        <NodeTiles
          nodes={[...exits(nodes.length), ...nodes]}
          selected={chosen(direction, node)}
          // Источники мастер не называет: их тут один-два, и все только что добавлены.
          sourceName={() => ""}
          hidden={false}
          rates={{}}
          onSelect={(name) => {
            const next = choice(name);
            onChange(next.direction, next.node ?? null);
          }}
        />
      </div>
      {nodes.length === 0 && (
        <Text tone="muted" size="xs" className="block">
          {t("Add a source first — then AUTO and nodes can be chosen.")}
        </Text>
      )}
    </div>
  );
}
