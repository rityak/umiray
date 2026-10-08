import { Gauge, SlidersHorizontal } from "lucide-react";
import { useEffect, useState } from "react";
import { Button, Callout, ChoiceCards, Dialog, Stepper, Toaster, toast } from "rootik";
import * as api from "../api";
import { MODES } from "../connection/ConnectionPath";
import { available, has } from "../features";
import { t, tk } from "../i18n";
import * as qd from "../qd/api";
import { failure, type Message } from "../shell/Banner";
import Disputed, { ADVISED, type Choices } from "./Disputed";
import RouteStep from "./RouteStep";
import SourceStep from "./SourceStep";
import Tuning, { TUNING, tunings } from "./Tuning";

type Way = "recommended" | "manual";
type StepId = "way" | "options" | "source" | "capture" | "route";

/// Шаги мастера (D-162) — список: новый шаг — запись здесь и его вид ниже. Условие шага —
/// ядро, которое мастер настраивает (перехват и маршрут есть только у mihomo), и способ:
/// спорные опции спрашивает только «Рекомендованная» (D-169).
const STEPS: { id: StepId; label: string; shows: (engine: api.Engine, way: Way) => boolean }[] = [
  { id: "way", label: tk("Approach"), shows: () => true },
  {
    id: "options",
    label: tk("Fine-tuning"),
    shows: (engine, way) => engine === "mihomo" && way === "recommended",
  },
  { id: "source", label: tk("Subscription"), shows: () => true },
  { id: "capture", label: tk("Capture"), shows: (engine) => engine === "mihomo" },
  { id: "route", label: tk("Route"), shows: (engine) => engine === "mihomo" },
];

type Props = {
  /// Мастер уже проходили — открыт снова из настроек. Тогда по умолчанию «Настрою сам»:
  /// «Далее» на «Рекомендованной» молча перемерило бы и переписало DNS, выбранный раньше.
  again: boolean;
  status: api.Status;
  sources: api.Source[];
  /// Перехват, выбранный сейчас: мастер начинает с него.
  mode: api.Choice;
  onLink: (input: string) => Promise<Message>;
  onFile: () => Promise<Message | null>;
  onElevate: () => void;
  onMode: (mode: api.Choice) => Promise<void>;
  onStatus: (status: api.Status) => void;
  /// Мастер пройден: запомнить это и какое ядро настроено.
  onDone: (engine: api.Engine) => Promise<void>;
  onConnect: () => Promise<void>;
  /// Закрыть без «Готово» — «настрою сам» (D-162).
  onClose: () => void;
};

/**
 * Мастер первого запуска (D-162). Каждый шаг применяет своё на «Далее»: подбор мерит и пишет,
 * перехват ставится, маршрут выбирается и подключает. Брошенный на середине мастер оставляет
 * пройденное — базовый конфиг клиент кладёт ещё под заставкой, и он рабочий сам по себе.
 */
export default function Setup({
  again,
  status,
  sources,
  mode: current,
  onLink,
  onFile,
  onElevate,
  onMode,
  onStatus,
  onDone,
  onConnect,
  onClose,
}: Props) {
  const [way, setWay] = useState<Way>(again ? "manual" : "recommended");
  /// Блокировать рекламу (D-169): DNS только из блокирующих и готовый набор правил. Три
  /// категории фильтра остаются в «Настройках mihomo» — мастеру хватает «да» или «нет».
  const [ads, setAds] = useState(false);
  const dnsFilter: api.DnsFilter = ads ? "ads" : "clean";
  /// Что набор правил уже получил: снятая галка трогает правила, только если её ставили
  /// здесь же, — иначе повторный мастер снял бы блокировку, включённую в «Маршрутизации».
  const [adsSet, setAdsSet] = useState<boolean | null>(null);
  /// Спорные опции (D-169). Первый запуск начинает с совета, повторный — с того, что в файле.
  const [choices, setChoices] = useState<Choices>(ADVISED);
  const [mode, setMode] = useState<api.Choice>(current);
  const [direction, setDirection] = useState<api.Direction>("direct");
  const [node, setNode] = useState<string | null>(null);
  /// Какие подборы уже записаны и с каким фильтром: «Назад» и снова «Далее» не мерят
  /// второй раз то же самое, а смена фильтра перемеряет только DNS.
  const [tuned, setTuned] = useState<Partial<Record<api.Tuning, string>>>({});
  /// Добавлена ссылка qd — без источников mihomo мастер настраивает qd.
  const [qdLinked, setQdLinked] = useState(false);
  const [at, setAt] = useState(0);
  const [doing, setDoing] = useState<string | null>(null);
  const [failed, setFailed] = useState<string | null>(null);

  /// Открытый снова, мастер начинает с выбранного выхода, а не с DIRECT: иначе «Готово»
  /// молча сбросило бы настроенный узел.
  useEffect(() => {
    api.connectionSnapshot().then(
      (snapshot) => {
        setDirection(snapshot.direction);
        setNode(snapshot.node);
      },
      () => {},
    );
  }, []);

  useEffect(() => {
    if (!again) return;
    api.advancedGet().then(
      (options) =>
        setChoices({
          sniffer: options.sniffer,
          openNat: options.openNat,
        }),
      () => {},
    );
  }, [again]);

  const engine: api.Engine = qdLinked && sources.length === 0 ? "qd" : "mihomo";
  const steps = STEPS.filter((step) => step.shows(engine, way));
  const step = steps[Math.min(at, steps.length - 1)];
  const last = step === steps.at(-1);
  const connectable = engine === "mihomo" ? sources.length > 0 : status.elevated;

  const link = async (input: string) => {
    const message = await onLink(input);
    if (qd.isLink(input)) setQdLinked(true);
    return message;
  };

  /// «Рекомендованная» (D-105): замер плюс запись, по строке итога на подбор. Неудачный подбор
  /// мастер не останавливает — его строка уедет в итог.
  const tune = async () => {
    if (way !== "recommended") return;
    const told: api.Report[] = [];
    const done = { ...tuned };
    for (const item of tunings()) {
      const key = item.filtered ? dnsFilter : "";
      if (done[item.id] === key) continue;
      setDoing(t(item.doing));
      told.push(
        await api
          .diagApply(item.id, dnsFilter)
          .catch((e): api.Report => ({ tool: item.id, verdict: "bad", headline: failure(e).text })),
      );
      done[item.id] = key;
    }
    setTuned(done);
    if ((ads || adsSet === true) && adsSet !== ads) {
      setDoing(t("Saving…"));
      try {
        onStatus(await api.routingAdsSet(ads));
        setAdsSet(ads);
        told.push({
          tool: "ads",
          verdict: "ok",
          headline: ads ? t("Ads are blocked") : t("Ad blocking is off"),
        });
      } catch (e) {
        told.push({ tool: "ads", verdict: "bad", headline: failure(e).text });
      }
    }
    if (told.length === 0) return;
    // Одной строкой: что подобрано или что не вышло. Подробности — в «Настройках mihomo».
    const name = (report: api.Report) =>
      report.tool === "ads"
        ? t("ads")
        : (TUNING.find((item) => item.id === report.tool)?.title ?? report.tool);
    const failed = told.filter((report) => report.verdict !== "ok");
    toast({
      title: failed.length === 0 ? t("Settings picked") : t("Not everything was picked"),
      tone: failed.length === 0 ? "success" : "warn",
      description:
        failed.length === 0
          ? told.map(name).join(" · ")
          : t("Failed: {what}", { what: failed.map(name).join(", ") }),
    });
  };

  /// Что шаг применяет на «Далее» — список, как и сами шаги. Источники добавляются ещё
  /// в шаге, кнопкой «Добавить»: шагу маршрута нужны их узлы.
  const apply: Record<StepId, () => Promise<void>> = {
    way: tune,
    options: async () => {
      setDoing(t("Saving…"));
      await api.advancedSet({ ...(await api.advancedGet()), ...choices });
    },
    source: async () => {},
    capture: async () => {
      setDoing(t("Saving…"));
      await onMode(mode);
    },
    route: async () => {
      setDoing(t("Saving…"));
      onStatus(await api.directionSet(direction, node ?? undefined));
    },
  };

  const next = async () => {
    setFailed(null);
    try {
      await apply[step.id]();
      if (!last) {
        setAt(steps.indexOf(step) + 1);
        return;
      }
      await onDone(engine);
      if (connectable) await onConnect();
    } catch (e) {
      setFailed(failure(e).text);
    } finally {
      setDoing(null);
    }
  };

  const body: Record<StepId, React.ReactNode> = {
    way: (
      <div className="flex flex-col gap-4">
        <ChoiceCards<Way>
          // Два варианта на всю ширину: при умолчании в 200 px сетка заводила третью,
          // пустую колонку, и пара карточек жалась к левому краю.
          minWidth={280}
          aria-label={t("How to set up")}
          value={way}
          onChange={setWay}
          options={[
            {
              value: "recommended",
              label: t("Recommended"),
              icon: <Gauge />,
              // Подбора MTU на этой ОС нет (D-174) — и обещать его нечего.
              description: has("mtuProbe")
                ? t("Sets up the core for every mode and picks DNS and MTU for this computer.")
                : t("Sets up the core for every mode and picks DNS for this computer."),
            },
            {
              value: "manual",
              label: t("I'll set it up myself"),
              icon: <SlidersHorizontal />,
              description: t("Changes nothing. You can set everything up later in Settings."),
            },
          ]}
        />
        {way === "recommended" && <Tuning ads={ads} onAds={setAds} />}
        {/* «Ничего не меняет» — про этот выбор, а не про уже записанный подбор: отменить
            его нечем, и молчать об этом значило бы врать подписью карточки. */}
        {way === "manual" && Object.keys(tuned).length > 0 && (
          <Callout
            tone="info"
            title={t("What was already picked stays saved. Change it in Mihomo Settings.")}
          />
        )}
      </div>
    ),
    options: <Disputed value={choices} onChange={setChoices} />,
    source: (
      <SourceStep
        sources={sources}
        qd={qdLinked}
        onLink={link}
        onFile={onFile}
        onElevate={onElevate}
      />
    ),
    capture: (
      <ChoiceCards<api.Choice>
        aria-label={t("Capture")}
        value={mode}
        onChange={setMode}
        options={MODES.filter(available).map((item) => ({
          value: item.value,
          label: item.label,
          icon: item.icon,
          description: t(item.about),
          note:
            item.value === "tun" && !status.elevated ? t("Needs administrator rights") : undefined,
        }))}
      />
    ),
    route: (
      <RouteStep
        direction={direction}
        node={node}
        onChange={(next, picked) => {
          setDirection(next);
          setNode(picked);
        }}
      />
    ),
  };

  return (
    <Dialog
      open
      size="lg"
      title={t("umiray setup")}
      dismissible={false}
      // Esc и крестик закрывают мастер, как «Пропустить» (D-162), — и, как она, не посреди
      // замера: закрытый на середине подбор дописал бы своё уже после «настрою сам».
      onClose={() => doing === null && onClose()}
      footer={
        <>
          <Button variant="ghost" className="mr-auto" disabled={doing !== null} onClick={onClose}>
            {t("Skip setup")}
          </Button>
          {steps.indexOf(step) > 0 && (
            <Button disabled={doing !== null} onClick={() => setAt(steps.indexOf(step) - 1)}>
              {t("Back")}
            </Button>
          )}
          <Button variant="primary" loading={doing !== null} onClick={next}>
            {doing ??
              (!last
                ? t("Next")
                : connectable && status.active === null
                  ? t("Connect")
                  : t("Done"))}
          </Button>
        </>
      }
    >
      {/* Свои тосты на время мастера: модальное окно делает всё вне себя инертным, и тост
          из App, хоть и виден поверх, не закрывается крестиком (ROOTIK §2). */}
      <Toaster position="bottom-right" />
      <div className="flex min-h-0 flex-col gap-4">
        <Stepper
          size="sm"
          steps={steps.map((item) => ({ id: item.id, label: t(item.label) }))}
          current={steps.indexOf(step)}
          linear
          onStepClick={(index) => doing === null && setAt(index)}
        />
        {failed && <Callout tone="danger" title={failed} />}
        {body[step.id]}
      </div>
    </Dialog>
  );
}
