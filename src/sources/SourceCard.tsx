import { RefreshCw, Trash2, X } from "lucide-react";
import { Badge, Button, Card, ConfirmButton, IconButton, Item, ItemGroup, Text } from "rootik";
import type * as api from "../api";
import { updatedLabel } from "../api";
import { t, tn } from "../i18n";
import Flag from "../shell/Flag";
import { hide } from "../shell/secret";

type Props = {
  source: api.Source;
  /// Privacy mode hides subscription names and addresses (D-127).
  hidden: boolean;
  /// Source nodes provide protocol counts and the expandable list.
  nodes: api.Node[];
  open: boolean;
  busy: boolean;
  onToggle: () => void;
  onRefresh: () => void;
  onRemove: () => void;
  /// Only record sources allow removal: subscription nodes return on refresh (D-121).
  onDropNode: (node: string) => void;
  note?: string;
};

/// Hide the token-bearing URL tail in screenshots and screen sharing (D-081).
function shortUrl(url: string): string {
  try {
    const parsed = new URL(url);
    const tail = `${parsed.pathname}${parsed.search}`.replace(/^\//, "");
    return tail.length > 6 ? `${parsed.host}/${tail.slice(0, 4)}…` : `${parsed.host}/${tail}`;
  } catch {
    return url.length > 28 ? `${url.slice(0, 26)}…` : url;
  }
}

/// Sort protocol counts with the source's most common protocol first.
function protocols(nodes: api.Node[]): { kind: string; count: number }[] {
  const seen = new Map<string, number>();
  for (const node of nodes) seen.set(node.kind, (seen.get(node.kind) ?? 0) + 1);
  return [...seen.entries()]
    .map(([kind, count]) => ({ kind, count }))
    .sort((a, b) => b.count - a.count);
}

/**
 * Expand a source to inspect its nodes, protocols and refresh time.
 */
export default function SourceCard({
  source,
  hidden,
  nodes,
  open,
  busy,
  onToggle,
  onRefresh,
  onRemove,
  onDropNode,
  note,
}: Props) {
  const kinds = protocols(nodes);

  return (
    <Card
      collapsible
      open={open}
      onOpenChange={onToggle}
      headingLevel={3}
      title={
        <span className="inline-flex items-center gap-2">
          {hide(source.name, hidden)}
          <Badge size="sm" variant="outline">
            {source.url !== null
              ? t("subscription")
              : source.records
                ? t("custom nodes")
                : t("my links")}
          </Badge>
        </span>
      }
      description={
        <>
          {tn(source.nodes, "{n} node", "{n} nodes")} ·{" "}
          {source.url !== null
            ? [updatedLabel(source), note ?? hide(shortUrl(source.url), hidden)]
                .filter(Boolean)
                .join(" · ")
            : source.records
              ? t("added here manually or from a file")
              : t("collected from individual links")}
        </>
      }
      actions={
        <>
          {source.url && (
            <Button size="sm" icon={<RefreshCw />} loading={busy} onClick={onRefresh}>
              {t("Refresh")}
            </Button>
          )}
          {/* Removal also deletes node overrides; require a second click. */}
          <ConfirmButton
            size="sm"
            variant="ghost"
            icon={<Trash2 />}
            confirmLabel={t("Delete for sure?")}
            onConfirm={onRemove}
          >
            {t("Delete")}
          </ConfirmButton>
        </>
      }
    >
      <div className="flex flex-col gap-2">
        {kinds.length > 0 && (
          <span className="flex flex-wrap gap-1">
            {kinds.map((item) => (
              <Badge key={item.kind} size="sm" variant="outline">
                {item.kind} · {item.count}
              </Badge>
            ))}
          </span>
        )}
        {nodes.length === 0 ? (
          <Text tone="muted" size="xs" className="block">
            {t("No nodes — refresh the source or inspect its response in Code view.")}
          </Text>
        ) : (
          <ItemGroup variant="divided" maxHeight={256}>
            {nodes.map((node) => (
              <Item
                key={node.name}
                size="sm"
                media={<Flag country={node.country} />}
                title={hide(node.name, hidden)}
                description={`${node.kind}${node.address !== null ? ` · ${hide(node.address, hidden)}` : ""}`}
                meta={
                  !node.supported && (
                    <Badge size="sm" tone="danger">
                      {t("unsupported")}
                    </Badge>
                  )
                }
                actions={
                  source.records && (
                    <IconButton
                      size="sm"
                      variant="ghost"
                      icon={<X />}
                      label={
                        hidden ? t("Remove node") : t("Remove node {name}", { name: node.name })
                      }
                      disabled={busy}
                      onClick={() => onDropNode(node.name)}
                    />
                  )
                }
              />
            ))}
          </ItemGroup>
        )}
      </div>
    </Card>
  );
}
