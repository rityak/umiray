import { AppWindow, PencilLine } from "lucide-react";
import { useMemo, useState } from "react";
import { Badge, Dialog, EmptyState, Item, ItemGroup, SearchInput } from "rootik";
import { useCached } from "../hooks/useCached";
import { unchanged, usePoll } from "../hooks/usePoll";
import { t } from "../i18n";

export type Process = {
  name: string;
  path?: string;
  icon?: string;
  connections?: number;
};

type Props = {
  taken: Set<string>;
  onPick: (pick: { process: string; path?: string }) => void;
  onClose: () => void;
  load: () => Promise<Process[]>;
  cacheKey: string;
  title: string;
  searchLabel: string;
};

const keyOf = (item: { name?: string; process?: string; path?: string }) =>
  (item.path || item.name || item.process || "").toLowerCase();

export default function ProcessPicker({
  taken,
  onPick,
  onClose,
  load,
  cacheKey,
  title,
  searchLabel,
}: Props) {
  const [running, setRunning] = useCached<Process[]>(cacheKey, []);
  const [term, setTerm] = useState("");

  usePoll(
    () => {
      load().then(unchanged(setRunning), () => {});
    },
    true,
    5000,
  );

  const needle = term.trim().toLowerCase();

  const shown = useMemo(() => {
    const folded = new Map<string, Process & { instances: number }>();
    for (const item of running) {
      if (
        needle &&
        !item.name.toLowerCase().includes(needle) &&
        !(item.path ?? "").toLowerCase().includes(needle)
      ) {
        continue;
      }
      const key = keyOf(item);
      const seen = folded.get(key);
      if (seen) {
        seen.instances += 1;
        seen.connections = (seen.connections ?? 0) + (item.connections ?? 0);
        seen.icon = seen.icon || item.icon;
        continue;
      }
      folded.set(key, { ...item, instances: 1 });
    }
    return [...folded.values()].sort(
      (a, b) => (b.connections ?? 0) - (a.connections ?? 0) || a.name.localeCompare(b.name),
    );
  }, [running, needle]);

  const listed = running.some((item) => item.name.toLowerCase() === needle);

  const take = (pick: { process: string; path?: string }) => {
    onPick(pick);
    onClose();
  };

  return (
    <Dialog open size="md" title={title} onClose={onClose}>
      <div className="flex flex-col gap-3">
        <SearchInput
          autoFocus
          aria-label={searchLabel}
          placeholder={searchLabel}
          value={term}
          onChange={(event) => setTerm(event.target.value)}
          onClear={() => setTerm("")}
        />
        {shown.length === 0 && !needle ? (
          <EmptyState size="sm" icon={<AppWindow />} title={t("No running apps")} />
        ) : (
          <ItemGroup variant="divided" maxHeight={360}>
            {shown.map((item) => {
              const ruled = taken.has(keyOf(item)) || taken.has(item.name.toLowerCase());
              return (
                <Item
                  key={keyOf(item)}
                  size="sm"
                  disabled={ruled}
                  onClick={() => take({ process: item.name, path: item.path })}
                  media={
                    item.icon ? (
                      <img src={item.icon} alt="" width={20} height={20} />
                    ) : (
                      <AppWindow size={20} aria-hidden="true" />
                    )
                  }
                  title={item.instances > 1 ? `${item.name} ×${item.instances}` : item.name}
                  description={item.path}
                  meta={
                    ruled ? (
                      <Badge size="sm">{t("in rules")}</Badge>
                    ) : item.connections ? (
                      <Badge size="sm" tone="accent">
                        {t("connections {n}", { n: item.connections })}
                      </Badge>
                    ) : undefined
                  }
                />
              );
            })}
            {needle && !listed && (
              <Item
                size="sm"
                disabled={taken.has(needle)}
                onClick={() => take({ process: term.trim() })}
                media={<PencilLine size={20} aria-hidden="true" />}
                title={term.trim()}
                description={t("not running — the rule matches the process name")}
                meta={taken.has(needle) ? <Badge size="sm">{t("in rules")}</Badge> : undefined}
              />
            )}
          </ItemGroup>
        )}
      </div>
    </Dialog>
  );
}
