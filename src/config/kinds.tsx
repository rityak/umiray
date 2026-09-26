import {
  ArrowUpRight,
  Ban,
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

/// Where a rule's traffic goes. Exactly three colours, by outcome: through VPN — accent,
/// bypass — neutral, block — danger. Whether it is a group or a server is a kind; the icon says it.
export function targetLook(
  target: string,
  nodes: string[],
): { Icon: LucideIcon; tone: Tone; word: string } {
  if (target === "DIRECT") return { Icon: ArrowUpRight, tone: "neutral", word: t("bypass VPN") };
  if (target === "REJECT") return { Icon: Ban, tone: "danger", word: t("block") };
  if (target === "umiray" || target === "AUTO")
    return { Icon: Globe, tone: "accent", word: t("through VPN") };
  if (nodes.includes(target))
    return { Icon: Server, tone: "accent", word: t("node · through VPN") };
  return { Icon: Layers, tone: "accent", word: t("group · through VPN") };
}
