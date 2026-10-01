import { Lock } from "lucide-react";
import { useState } from "react";
import { Badge, Card, Divider, KeyValue, Text, type Tone } from "rootik";
import { t } from "../i18n";
import { HEALTH } from "./GroupRow";
import type { Choices } from "./groups";
import { GROUP_KIND } from "./kinds";

type Props = {
  choices: Choices;
  /// Names of the user's groups: inside the alias they are items, in the same order as in
  /// the list above.
  mine: string[];
};

/**
 * The two groups the client assembles itself (D-053) — for reference.
 *
 * They sit **below** the "+ Group" button and are dimmed: nobody opens the section for them.
 * But knowing that the names `AUTO` and `umiray` are taken, and what they are made of, is
 * needed before creating your own group with the same name.
 */
export default function BuiltinGroups({ choices, mine }: Props) {
  const [open, setOpen] = useState<string | null>(null);
  const total = choices.nodes.filter((node) => node.supported).length;

  const groups = [
    {
      id: "AUTO",
      kind: "load-balance",
      summary: t("all nodes of all sources · {n}", { n: total }),
      spec: [
        [t("type"), "load-balance"],
        [t("strategy"), t("consistent-hashing — one address always through one server")],
        [t("check"), t("{url} · every 300 s", { url: HEALTH })],
        [t("members"), t("all nodes of all sources, as a live list")],
      ],
      order: [] as { name: string; tone: Tone }[],
      note: t("The client uses this name. Create your own group called AUTO and yours wins."),
    },
    {
      id: "umiray",
      kind: "select",
      summary: t("the exit MATCH points at"),
      spec: [
        [t("type"), "select"],
        [t("who picks"), t("the choice in Connection — it points this group")],
        [t("why"), t("the target of the final MATCH rule: whatever nothing else caught goes here")],
      ],
      order: [
        { name: "AUTO", tone: "accent" as Tone },
        ...choices.sources.map((source) => ({
          name: t("nodes of «{source}»", { source: source.name }),
          tone: "neutral" as Tone,
        })),
        ...mine.map((name) => ({ name, tone: "accent" as Tone })),
        { name: "DIRECT", tone: "neutral" as Tone },
      ],
      note: t("Your groups, in the same order as the list above."),
    },
  ];

  return (
    <>
      <Divider label={t("names taken by the client")} />
      {groups.map((group) => (
        <Card
          key={group.id}
          variant="outline"
          collapsible
          open={open === group.id}
          onOpenChange={(next) => setOpen(next ? group.id : null)}
          headingLevel={3}
          icon={(() => {
            const Icon = GROUP_KIND[group.kind]?.Icon;
            return Icon ? <Icon /> : undefined;
          })()}
          title={group.id}
          description={`${group.kind} · ${group.summary}`}
          actions={
            <Badge size="sm" variant="outline" icon={<Lock />}>
              {t("assembled by the client")}
            </Badge>
          }
        >
          <div className="flex flex-col gap-3">
            <KeyValue items={group.spec.map(([label, value]) => ({ label, value }))} />
            {group.order.length > 0 && (
              <div className="flex flex-wrap items-center gap-1.5">
                {group.order.map((item) => (
                  <Badge key={item.name} size="sm" variant="outline" tone={item.tone}>
                    {item.name}
                  </Badge>
                ))}
              </div>
            )}
            <Text tone="muted" size="xs" className="block">
              {group.note}
            </Text>
          </div>
        </Card>
      ))}
    </>
  );
}
