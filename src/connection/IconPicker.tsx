import { icons, type LucideIcon } from "lucide-react";
import { useMemo, useState } from "react";
import { Button, IconButton, Popover, SearchInput, Tabs, Text } from "rootik";
import { t } from "../i18n";
import { FLAG_CODES, FlagSvg } from "../shell/Flag";
import GroupIcon from "./GroupIcon";
import { regionName } from "./groups";

type Tab = "icons" | "flags";

/// What the icon tab shows before a search: things a group is usually about.
const POPULAR = [
  "Globe",
  "Flag",
  "Shield",
  "ShieldCheck",
  "Zap",
  "Server",
  "Layers",
  "Star",
  "Scale",
  "Rocket",
  "Gauge",
  "Lock",
  "Cloud",
  "Wifi",
  "Smartphone",
  "Laptop",
  "Monitor",
  "Gamepad2",
  "Film",
  "Tv",
  "Music",
  "MessageCircle",
  "Send",
  "Mail",
  "Briefcase",
  "House",
  "Building2",
  "Plane",
  "Car",
  "TrainFront",
  "Ship",
  "Mountain",
  "Trees",
  "Sun",
  "Moon",
  "Heart",
  "Crown",
  "Gem",
  "Bot",
  "Brain",
  "Code",
  "Terminal",
  "Database",
  "Network",
  "Router",
  "Radio",
  "Satellite",
  "Anchor",
  "Compass",
  "MapPin",
  "Ghost",
  "Flame",
  "Snowflake",
  "Leaf",
  "Coffee",
  "ShoppingCart",
  "CreditCard",
  "Bitcoin",
  "Users",
  "GraduationCap",
  "BookOpen",
  "Newspaper",
  "Camera",
  "Headphones",
  "Phone",
  "Video",
  "Download",
  "Infinity",
  "Hash",
  "Bookmark",
  "Tag",
  "Box",
  "Puzzle",
  "Target",
  "Trophy",
  "Swords",
  "Hand",
  "Shuffle",
  "Timer",
  "Clock",
].filter((name) => name in icons);

const ALL = Object.keys(icons);

/// A search over 1700 icons draws only the first matches: the grid is for picking, and
/// a longer list is a hint to type more.
const LIMIT = 160;

type Props = {
  value: string | null;
  /// The kind's own icon, shown while nothing is chosen.
  fallback: LucideIcon;
  label: string;
  onChange: (id: string | null) => void;
};

/// Pick a group's icon (D-172): any lucide icon or a country flag, with a search.
export default function IconPicker({ value, fallback, label, onChange }: Props) {
  const [open, setOpen] = useState(false);
  const [tab, setTab] = useState<Tab>(value?.startsWith("flag:") ? "flags" : "icons");
  const [query, setQuery] = useState("");
  const wanted = query.trim().toLowerCase();

  const names = useMemo(
    () => (wanted === "" ? POPULAR : ALL.filter((name) => name.toLowerCase().includes(wanted))),
    [wanted],
  );
  const flags = useMemo(
    () =>
      FLAG_CODES.map((code) => ({ code, name: regionName(code) }))
        .filter(
          (flag) =>
            wanted === "" ||
            flag.code.toLowerCase() === wanted ||
            flag.name.toLowerCase().includes(wanted),
        )
        .sort((a, b) => a.name.localeCompare(b.name)),
    [wanted],
  );

  const pick = (id: string | null) => {
    onChange(id);
    setOpen(false);
  };

  return (
    <Popover
      open={open}
      onOpenChange={setOpen}
      trigger={
        <IconButton
          size="sm"
          variant="ghost"
          icon={<GroupIcon id={value} fallback={fallback} />}
          label={label}
        />
      }
    >
      <div className="flex w-80 flex-col gap-2">
        <Tabs<Tab>
          size="sm"
          fill
          aria-label={t("Icon kind")}
          items={[
            { value: "icons", label: t("Icons") },
            { value: "flags", label: t("Flags") },
          ]}
          value={tab}
          onChange={setTab}
        />
        <SearchInput
          size="sm"
          value={query}
          placeholder={tab === "icons" ? t("Search in English: shield, game…") : t("Country")}
          onChange={(event) => setQuery(event.target.value)}
          onClear={() => setQuery("")}
        />
        <div className="grid max-h-64 grid-cols-8 content-start gap-1 overflow-y-auto">
          {tab === "icons"
            ? names.slice(0, LIMIT).map((name) => {
                const Icon = icons[name as keyof typeof icons];
                return (
                  <IconButton
                    key={name}
                    size="sm"
                    variant={value === `lucide:${name}` ? "primary" : "ghost"}
                    icon={<Icon />}
                    label={name}
                    onClick={() => pick(`lucide:${name}`)}
                  />
                );
              })
            : flags.map((flag) => (
                <IconButton
                  key={flag.code}
                  size="sm"
                  variant={value === `flag:${flag.code.toLowerCase()}` ? "primary" : "ghost"}
                  icon={<FlagSvg code={flag.code} />}
                  label={flag.name}
                  onClick={() => pick(`flag:${flag.code.toLowerCase()}`)}
                />
              ))}
        </div>
        {tab === "icons" && names.length > LIMIT && (
          <Text tone="muted" size="xs">
            {t("{n} more — narrow the search", { n: names.length - LIMIT })}
          </Text>
        )}
        {value !== null && (
          <Button size="sm" variant="ghost" className="self-start" onClick={() => pick(null)}>
            {t("Default icon")}
          </Button>
        )}
      </div>
    </Popover>
  );
}
