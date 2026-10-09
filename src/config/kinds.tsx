import {
  ArrowUpRight,
  Ban,
  CircleAlert,
  Gauge,
  Globe,
  Hand,
  Layers,
  ListOrdered,
  type LucideIcon,
  Scale,
  Server,
} from "lucide-react";
import type { Tone } from "rootik";
import { t, tk } from "../i18n";

/// A group's type is a kind, not a meaning: the icon carries it, it has no colour (STYLEGUIDE,
/// "Design language"). Colour in the window is taken by where traffic goes and its state.
export const GROUP_KIND: Record<string, { Icon: LucideIcon; word: string }> = {
  select: { Icon: Hand, word: tk("manual choice") },
  "url-test": { Icon: Gauge, word: tk("fastest") },
  fallback: { Icon: ListOrdered, word: tk("first alive") },
  "load-balance": { Icon: Scale, word: tk("load balancing") },
};

export const UNKNOWN_KIND = { Icon: Layers, word: tk("no type") };

/// A target nothing answers to: a renamed or deleted group, a node the subscription dropped.
/// The build sends such a rule to `umiray` (D-156) — the window says so instead of "group".
/// A VOLT exit is lost only while the bypass is off, and the hint says that (D-191).
export function lostLook(target = ""): { Icon: LucideIcon; tone: Tone; word: string } {
  return {
    Icon: CircleAlert,
    tone: "warn",
    word: ["DIRECT-VOLT", "DIRECT-AUTO"].includes(target)
      ? t("bypass is off · goes to the exit chosen in Connection")
      : t("not found · goes to the exit chosen in Connection"),
  };
}

/// Where a rule's traffic goes. Exactly three colours, by outcome: through VPN — accent,
/// bypass — neutral, block — danger. Whether it is a group or a server is a kind; the icon says it.
export function targetLook(
  target: string,
  nodes: string[],
): { Icon: LucideIcon; tone: Tone; word: string } {
  if (["DIRECT", "DIRECT-PLAIN", "DIRECT-VOLT", "DIRECT-AUTO"].includes(target))
    return { Icon: ArrowUpRight, tone: "neutral", word: t("bypass proxy") };
  if (target === "REJECT") return { Icon: Ban, tone: "danger", word: t("block") };
  // `umiray` follows the choice in Connection, and that choice can be DIRECT: "through
  // VPN" would be a lie there.
  if (target === "umiray")
    return { Icon: Globe, tone: "accent", word: t("whatever you pick in Connection") };
  if (target === "AUTO") return { Icon: Globe, tone: "accent", word: t("through proxy") };
  if (nodes.includes(target))
    return { Icon: Server, tone: "accent", word: t("node · through proxy") };
  return { Icon: Layers, tone: "accent", word: t("group · through proxy") };
}
