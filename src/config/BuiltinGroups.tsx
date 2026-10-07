import { ChevronDown, ChevronUp, Globe, Lock } from "lucide-react";
import { useEffect, useState } from "react";
import { Badge, Card, Divider, IconButton, KeyValue, Text, type Tone } from "rootik";
import * as api from "../api";
import { groupName, iconOf } from "../connection/groups";
import IconPicker from "../connection/IconPicker";
import { t, tn } from "../i18n";
import type { Choices } from "./groups";
import { GROUP_KIND, UNKNOWN_KIND } from "./kinds";

type Props = {
  choices: Choices;
  /// Names of the user's groups: inside the alias they are items, in the same order as in
  /// the list above.
  mine: string[];
  /// Shared with Connection (`Settings.group_icons`, D-172).
  icons: Record<string, string>;
  onIcon: (name: string, id: string | null) => void;
};

/// What the build tells about a client group: where its members come from.
function membersOf(name: string): string {
  if (name === "AUTO")
    return t("all nodes except the ones taken out in Connection, as a live list");
  if (name === "umiray-udp") return t("nodes whose protocol carries UDP itself, not inside TCP");
  if (name.startsWith("umiray-geo-")) return t("nodes of one country, two or more");
  if (name.startsWith("umiray-proto-")) return t("nodes of one protocol, two or more");
  return t("assembled by the client");
}

/**
 * The groups the client assembles itself (D-053, D-113, D-172) — under their names for people,
 * with the name in the core config next to it: what is in the build is what is shown here.
 *
 * Below the user's groups: nobody opens the section for them, but their names are taken and
 * a rule can lead to them.
 */
export default function BuiltinGroups({ choices, mine, icons, onIcon }: Props) {
  const [open, setOpen] = useState<string | null>(null);
  const [built, setBuilt] = useState<api.BuiltGroup[]>([]);
  const [check, setCheck] = useState<{ url: string; every: number } | null>(null);
  useEffect(() => {
    api.connectionSnapshot().then(
      (snapshot) => setBuilt(snapshot.groups),
      () => setBuilt([]),
    );
    Promise.all([api.clientHealthGet(), api.clientHealthIntervalGet()]).then(
      ([url, every]) => setCheck({ url, every }),
      () => setCheck(null),
    );
  }, []);

  const total = choices.nodes.filter((node) => node.supported).length;
  const checkLine = check && t("{url} · every {n} s", { url: check.url, n: check.every });

  const groups = [
    ...built
      .filter((group) => group.name === "AUTO")
      .map((group) => ({
        id: group.name,
        kind: group.kind,
        summary: t("{n} of {total} nodes", { n: group.members.length, total }),
        spec: [
          [t("strategy"), t("consistent-hashing — one address always through one server")],
          [t("members"), membersOf(group.name)],
        ],
        order: [] as { name: string; tone: Tone }[],
        pickable: true,
      })),
    {
      id: "umiray",
      kind: "select",
      summary: t("where MATCH and «Selected exit» rules lead"),
      spec: [
        [t("who picks"), t("the choice in Connection — switched live, without reloading the core")],
        [
          t("why"),
          t(
            "rules pointing here follow your choice in Connection, so changing it rewrites nothing",
          ),
        ],
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
      pickable: false,
    },
    ...built
      .filter((group) => group.name !== "AUTO")
      .map((group) => ({
        id: group.name,
        kind: group.kind,
        summary: tn(group.members.length, "{n} node", "{n} nodes"),
        spec: [
          [t("members"), membersOf(group.name)],
          [t("who turns it on"), t("Auto groups on the Groups tab of Connection")],
        ],
        order: group.members.map((name) => ({ name, tone: "neutral" as Tone })),
        pickable: true,
      })),
  ];

  return (
    <>
      <Divider label={t("assembled by the client")} />
      {groups.map((group) => {
        const look = GROUP_KIND[group.kind] ?? UNKNOWN_KIND;
        const label = groupName(group.id, choices.nodes);
        const shown = open === group.id;
        return (
          <Card
            key={group.id}
            variant="outline"
            headingLevel={3}
            media={
              group.pickable ? (
                <IconPicker
                  value={iconOf(
                    { name: group.id, kind: group.kind, members: [], own: false },
                    icons,
                  )}
                  fallback={look.Icon}
                  label={t("Icon of {name}", { name: label })}
                  onChange={(id) => onIcon(group.id, id)}
                />
              ) : undefined
            }
            icon={group.pickable ? undefined : <Globe />}
            title={label}
            description={`${group.kind} · ${group.summary}`}
            actions={
              <span className="flex items-center gap-2">
                <Badge size="sm" variant="outline" icon={<Lock />}>
                  {t("assembled by the client")}
                </Badge>
                <IconButton
                  size="sm"
                  variant="ghost"
                  icon={shown ? <ChevronUp /> : <ChevronDown />}
                  label={shown ? t("Fold") : t("Show details")}
                  aria-expanded={shown}
                  onClick={() => setOpen(shown ? null : group.id)}
                />
              </span>
            }
          >
            {shown && (
              <div className="um-swap flex flex-col gap-3">
                <KeyValue
                  items={[
                    { label: t("name in the core config"), value: group.id },
                    { label: t("type"), value: group.kind },
                    ...group.spec.map(([label, value]) => ({ label, value })),
                    ...(group.kind !== "select" && checkLine
                      ? [{ label: t("check"), value: checkLine }]
                      : []),
                  ]}
                />
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
                  {t("The name is taken by the client: a group of yours can't use it.")}
                </Text>
              </div>
            )}
          </Card>
        );
      })}
    </>
  );
}
