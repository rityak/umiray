import { RefreshCw } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Badge, Card, DataTable, EmptyState, IconButton, KeyValue, Switch } from "rootik";
import * as api from "../api";
import { t } from "../i18n";

/**
 * Отладка клиента (D-123): что он сделал сам, отдельно от вывода ядра.
 *
 * Хуки жизненного цикла (D-101) пишут по строке на шаг — фаза, шаг, время, исход — и
 * ложатся они в то же кольцо, что и вывод ядра. В «Логах» они тонут среди его строк;
 * здесь остаются только они, разобранные по столбцам.
 *
 * Рядом — состояние, которое клиент держит о себе: то, что видно командам, но не видно
 * в окне. Отдельного сбора данных нет намеренно: показываем то, что уже есть, иначе
 * отладка заводит свою правду и расходится с настоящей.
 */
export default function Debug() {
  const [steps, setSteps] = useState<Step[]>([]);
  const [state, setState] = useState<Record<string, unknown> | null>(null);
  const [live, setLive] = useState(true);

  const read = useCallback(async () => {
    const [logs, status, nodes, sources] = await Promise.all([
      api.coreLogs("mihomo").catch(() => [] as string[]),
      api.coreStatus().catch(() => null),
      api.nodesList().catch(() => []),
      api.sourcesList().catch(() => []),
    ]);
    setSteps(logs.flatMap(parse));
    setState({
      [t("Core running")]: status?.running ?? "—",
      [t("Mode")]: status?.mode ?? "—",
      [t("Port")]: status?.port ?? "—",
      [t("Restart reason")]: status?.restartReason ?? t("none"),
      [t("Administrator rights")]: status?.elevated ?? "—",
      [t("Sources")]: sources.length,
      [t("Nodes")]: nodes.length,
      [t("Unsupported nodes")]: nodes.filter((node) => !node.supported).length,
      [t("Edited nodes")]: nodes.filter((node) => node.edited).length,
    });
  }, []);

  useEffect(() => {
    read();
    if (!live) return;
    const timer = setInterval(read, 2000);
    return () => clearInterval(timer);
  }, [read, live]);

  const rows = steps.map((step, at) => ({ ...step, at }));
  return (
    <div
      data-dev="view"
      // Та же прокручиваемая часть, что `shell/Scroll`: страница окна не прокручивается.
      className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto pb-1 [contain:layout]"
    >
      <Card
        title={t("Lifecycle hooks")}
        description={t("{n} steps", { n: steps.length })}
        actions={
          <>
            <Switch
              label={t("Follow")}
              checked={live}
              onChange={(event) => setLive(event.target.checked)}
            />
            <IconButton variant="ghost" icon={<RefreshCw />} label={t("Refresh")} onClick={read} />
          </>
        }
      >
        {steps.length === 0 ? (
          <EmptyState
            size="sm"
            title={t("No steps yet")}
            hint={t("Connect VPN to see core startup, shutdown and periodic steps")}
          />
        ) : (
          <DataTable
            density="compact"
            rows={rows}
            // Строки лога различаются только местом.
            rowKey={(row) => String(row.at)}
            columns={[
              { key: "phase", header: t("Phase"), mono: true, value: (row) => row.phase },
              {
                key: "id",
                header: t("Step"),
                mono: true,
                cell: (row) => <span data-step={row.id}>{row.id}</span>,
              },
              { key: "spent", header: t("Time"), mono: true, value: (row) => row.spent },
              {
                key: "outcome",
                header: t("Outcome"),
                cell: (row) => (
                  <Badge size="sm" tone={row.failed ? "danger" : "success"}>
                    {row.outcome}
                  </Badge>
                ),
              },
            ]}
          />
        )}
      </Card>

      <Card title={t("Client state")}>
        <KeyValue
          items={Object.entries(state ?? {}).map(([label, value]) => ({
            label,
            value: String(value),
          }))}
        />
      </Card>
    </div>
  );
}

type Step = { phase: string; id: string; spent: string; outcome: string; failed: boolean };

/// Строка хука: `umiray: start · config · 6 мс · ок`. Разбираем по той же разделительной
/// точке, которой её и собрали, — формат один и живёт в `app/lifecycle.rs`.
function parse(line: string): Step[] {
  const at = line.indexOf("umiray: ");
  if (at < 0) return [];
  const parts = line
    .slice(at + "umiray: ".length)
    .split("·")
    .map((part) => part.trim());
  if (parts.length < 4) return [];
  const [head, id, spent, ...rest] = parts;
  // Хвостовая кавычка — от `msg="…"` в строке ядра: строку хука оно заворачивает в свою.
  const outcome = rest.join(" · ").replace(/"\s*$/, "");
  return [
    {
      phase: head.split(/\s+/)[0],
      id,
      spent,
      outcome,
      failed: outcome.startsWith("отказ"),
    },
  ];
}
