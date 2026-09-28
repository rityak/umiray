import { FileCode2, Play, Terminal } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import {
  Button,
  Card,
  Checkbox,
  Field,
  Input,
  NavGroup,
  NavItem,
  NumberInput,
  Tooltip,
} from "rootik";
import * as api from "../api";
import { useCached } from "../hooks/useCached";
import { t } from "../i18n";
import { failure, type Message } from "../shell/Banner";
import Scroll from "../shell/Scroll";
import Console, { type Entry } from "./Console";
import Effective from "./Effective";
import ReportTable from "./ReportTable";
import Resolvers from "./Resolvers";
import Verdict from "./Verdict";

/// Собранный конфиг — не проба, а вид (D-130): отчёта и вердикта у него нет, поэтому
/// в списке утилит бэкенда его нет тоже, и пункт дописывает окно.
const CONFIG = "config";

type Props = {
  onMessage: (message: Message) => void;
};

/**
 * «Инструменты»: утилиты по одной, с параметрами, таблицей и сырым выводом (D-097, D-115).
 *
 * Единственный вид раздела. «Проверка» и «Отчёт» убраны: карточки отвечали словами
 * там, где ответ всё равно надо было читать в таблице, а отчёт складывал в текст прогон,
 * который никто не гонял целиком. Утилита, чей ответ нужен сам по себе, теперь стоит
 * хуком в том месте, где он нужен, — и её незачем нажимать (D-115).
 *
 * Опроса тут нет намеренно: пробы гоняются по нажатию и только. Диагностика, которая
 * ходит в сеть сама по себе, — это уже не диагностика, а фоновый трафик.
 */
export default function Tools({ onMessage }: Props) {
  const [tools, setTools] = useCached<api.Tool[]>("tools.list", []);
  const [reports, setReports] = useCached<Record<string, api.Report>>("tools.reports", {});
  const [entries, setEntries] = useCached<Entry[]>("tools.entries", []);
  const [running, setRunning] = useState<string | null>(null);
  const [picked, setPicked] = useCached<string | null>("tools.picked", null);
  const [domain, setDomain] = useState("");
  const [domains, setDomains] = useState("");
  const [host, setHost] = useState("");
  const [timeout, setWait] = useState<number | null>(null);
  const [all, setAll] = useState(false);
  const [core, setCore] = useState(true);

  useEffect(() => {
    api.diagTools().then(setTools, (e) => onMessage(failure(e)));
  }, [onMessage]);

  /// Запуск и «сделать» отличаются одной командой: и та и другая отдают отчёт, и обе
  /// пишут его в ту же консоль. Второй такой же обвязки заводить незачем — разъедется
  /// на первой же правке.
  const gone = useCallback(
    async (id: string, args: api.DiagArgs, apply: boolean) => {
      if (running !== null) return;
      setRunning(id);
      try {
        const report = await (apply ? api.diagApply(id, args) : api.diagRun(id, args));
        setReports((current) => ({ ...current, [id]: report }));
        setEntries((current) => [
          ...current,
          ...report.lines.map((line) => ({ tool: report.tool, line })),
        ]);
      } catch (e) {
        // Упавшая утилита — её собственная неудача, а не поломка раздела: причина
        // уезжает баннером и строкой в консоль.
        onMessage(failure(e));
        setEntries((current) => [
          ...current,
          { tool: id, line: { tone: "bad" as const, text: `${id}: ${api.asAppError(e).message}` } },
        ]);
      } finally {
        setRunning(null);
      }
    },
    [onMessage, running],
  );

  const tool =
    picked === CONFIG ? null : (tools.find((item) => item.id === picked) ?? tools[0] ?? null);
  const groups = [...new Set(tools.map((item) => item.group))];
  const report = tool ? reports[tool.id] : undefined;

  /// Параметры полосы — одни и те же для «Запустить» и для «Сделать»: действие меряет
  /// заново и обязано мерить ровно то же, что показала таблица.
  const parameters = () => {
    const args: api.DiagArgs = {};
    if (!tool) return args;
    if (tool.params.includes("domain") && domain.trim()) args.domain = domain.trim();
    if (tool.params.includes("domains") && domains.trim()) {
      args.domains = domains
        .split(",")
        .map((item) => item.trim())
        .filter(Boolean);
    }
    if (tool.params.includes("host") && host.trim()) args.host = host.trim();
    if (tool.params.includes("hosts") && host.trim()) {
      args.hosts = host
        .split(",")
        .map((item) => item.trim())
        .filter(Boolean);
    }
    if (tool.params.includes("timeout") && timeout !== null) args.timeoutMs = timeout;
    if (tool.params.includes("all") && all) args.all = true;
    if (tool.params.includes("core") && !core) args.core = false;
    return args;
  };

  const marked = report?.rows.some((row) => row.mark) ?? false;

  return (
    // Список утилит и сама утилита — каждая колонка со своей прокруткой: страница окна
    // не прокручивается (STYLEGUIDE, «Каркас»).
    <div className="grid min-h-0 flex-1 grid-cols-[220px_minmax(0,1fr)] grid-rows-[minmax(0,1fr)] gap-3 max-[819px]:grid-cols-1">
      <Card padding="sm" className="min-h-0">
        <nav
          aria-label={t("Utilities")}
          className="flex h-full min-h-0 flex-col gap-1 overflow-y-auto"
        >
          {groups.map((group) => (
            <NavGroup key={group} label={t(group)}>
              {tools
                .filter((item) => item.group === group)
                .map((item) => (
                  // Обёртка — ради подсказки набора: `NavItem` рисует свою системной.
                  <Tooltip key={item.id} content={t(item.hint)} placement="right">
                    <div>
                      <NavItem
                        icon={<Terminal />}
                        label={item.id}
                        active={picked !== CONFIG && tool?.id === item.id}
                        trailing={
                          reports[item.id] && <Verdict value={reports[item.id].verdict} hideLabel />
                        }
                        onClick={() => setPicked(item.id)}
                      />
                    </div>
                  </Tooltip>
                ))}
            </NavGroup>
          ))}
          <NavGroup label={t("Config")}>
            <NavItem
              icon={<FileCode2 />}
              label="config"
              active={picked === CONFIG}
              onClick={() => setPicked(CONFIG)}
            />
          </NavGroup>
        </nav>
      </Card>

      <Scroll>
        {picked === CONFIG ? (
          <Effective onMessage={onMessage} />
        ) : (
          <>
            {/* Параметры — только те, которые утилита правда принимает: список приходит
                от неё самой. */}
            <Card
              title={tool ? t(tool.title) : "—"}
              description={tool ? t(tool.hint) : undefined}
              actions={
                <>
                  {marked && tool && (
                    <Tooltip
                      content={t("Write the selected values to your document and apply them")}
                    >
                      <Button
                        disabled={running !== null}
                        onClick={() => gone(tool.id, parameters(), true)}
                      >
                        {t("Apply selected values")}
                      </Button>
                    </Tooltip>
                  )}
                  <Button
                    variant="primary"
                    icon={<Play />}
                    loading={running !== null}
                    disabled={tool === null}
                    onClick={() => tool && gone(tool.id, parameters(), false)}
                  >
                    {t("Run")}
                  </Button>
                </>
              }
            >
              <div className="flex flex-wrap items-end gap-3">
                {tool?.params.includes("domain") && (
                  <Field label={t("Domain")}>
                    <Input
                      value={domain}
                      placeholder="example.com"
                      onChange={(event) => setDomain(event.target.value)}
                    />
                  </Field>
                )}
                {tool?.params.includes("domains") && (
                  <Field label={t("Names")}>
                    <Input
                      value={domains}
                      placeholder={t("comma-separated")}
                      onChange={(event) => setDomains(event.target.value)}
                    />
                  </Field>
                )}
                {(tool?.params.includes("host") || tool?.params.includes("hosts")) && (
                  <Field label={tool.params.includes("hosts") ? t("Names") : t("Target")}>
                    <Input
                      value={host}
                      placeholder={tool.params.includes("hosts") ? t("comma-separated") : "1.1.1.1"}
                      onChange={(event) => setHost(event.target.value)}
                    />
                  </Field>
                )}
                {tool?.params.includes("timeout") && (
                  <Field label={t("Timeout")}>
                    <NumberInput
                      className="w-32"
                      allowEmpty
                      min={1}
                      unit={t("ms")}
                      placeholder="1500"
                      value={timeout}
                      onChange={setWait}
                    />
                  </Field>
                )}
                {tool?.params.includes("core") && (
                  <Checkbox
                    label={t("encrypted resolvers through the core")}
                    checked={core}
                    onChange={(event) => setCore(event.target.checked)}
                  />
                )}
                {tool?.params.includes("all") && (
                  <Checkbox
                    label={t("entire resolver catalog")}
                    checked={all}
                    onChange={(event) => setAll(event.target.checked)}
                  />
                )}
              </div>
            </Card>

            {report && (
              <Card padding="sm">
                <ReportTable report={report} />
              </Card>
            )}
            {tool?.id === "dns-race" && <Resolvers />}
            <Console entries={entries} running={running} onClear={() => setEntries([])} />
          </>
        )}
      </Scroll>
    </div>
  );
}
