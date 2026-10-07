import { icons, type LucideIcon } from "lucide-react";
import { FlagSvg } from "../shell/Flag";

// ponytail: the whole lucide set (~1800 icons) and every flag ride in the main bundle — a
// desktop window loads it from disk. Too slow to start → `lucide-react/dynamic` per icon.

/// The lucide icon behind an id like `lucide:Globe`, if there is one.
export function lucideOf(id: string | null): LucideIcon | undefined {
  if (!id?.startsWith("lucide:")) return undefined;
  return icons[id.slice("lucide:".length) as keyof typeof icons];
}

/// A group's icon (D-172): `lucide:<Name>` or `flag:<code>`; nothing chosen — `fallback`,
/// the kind's own icon.
export default function GroupIcon({ id, fallback }: { id: string | null; fallback: LucideIcon }) {
  if (id?.startsWith("flag:")) return <FlagSvg code={id.slice("flag:".length)} />;
  const Icon = lucideOf(id) ?? fallback;
  return <Icon />;
}
