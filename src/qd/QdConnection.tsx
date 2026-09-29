import { Download, Server, ShieldAlert } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { Button, Callout, Card, EmptyState, Field, Item, SegmentedControl } from "rootik";
import type * as api from "../api";
import NodeDelay from "../connection/NodeDelay";
import Nodes from "../connection/Nodes";
import PowerRow from "../connection/PowerRow";
import Speed from "../connection/Speed";
import { ENGINES } from "../engines";
import { useCached } from "../hooks/useCached";
import { unchanged, usePoll } from "../hooks/usePoll";
import { type Speed as Rate, ZERO } from "../hooks/useTraffic";
import { t } from "../i18n";
import { failure, type Message, notice } from "../shell/Banner";
import Uptime from "../shell/Uptime";
import * as qd from "./api";

type Wish = { egress?: boolean; adblock?: boolean };

type Props = {
  status: qd.Status | null;
  /// When the tunnel came up — counted by the client, not here: the window outlives it.
  started: number | null;
  /// Another engine carries traffic now; turning qd on stops it first (D-154).
  other: api.Engine | null;
  powering: boolean;
  onPower: () => void;
  onChanged: () => Promise<void> | void;
  onInstall: () => void;
  onElevate: () => void;
  onAdd: () => void;
  onMessage: (message: Message) => void;
};

export default function QdConnection({
  status,
  started,
  other,
  powering,
  onPower,
  onChanged,
  onInstall,
  onElevate,
  onAdd,
  onMessage,
}: Props) {
  const [nodes, setNodes] = useCached<qd.Node[]>("qd.nodes", []);
  const [history, setHistory] = useCached<Rate[]>("qd.history", []);
  const [wish, setWish] = useState<Wish>({});
  const state = status?.state ?? null;
  const connected = state?.connected ?? false;

  const reloadNodes = useCallback(() => {
    qd.nodes().then(unchanged(setNodes), () => {});
  }, []);
  usePoll(reloadNodes, Boolean(state));
  usePoll(() => {
    qd.history(1).then(
      (got) => setHistory(got.points.map((point) => ({ down: point.down, up: point.up }))),
      () => {},
    );
  }, connected);

  const flip = useCallback(
    async (patch: Wish) => {
      setWish((was) => ({ ...was, ...patch }));
      try {
        await qd.toggle(patch);
        await onChanged();
      } catch (e) {
        onMessage(failure(e));
      } finally {
        setWish((was) => {
          const rest = { ...was };
          for (const key of Object.keys(patch) as (keyof Wish)[]) delete rest[key];
          return rest;
        });
      }
    },
    [onChanged, onMessage],
  );
  const egress = wish.egress ?? state?.egress ?? false;
  const adblock = wish.adblock ?? state?.adblock ?? false;

  const shown = useMemo(() => nodes.map(qd.asNode), [nodes]);
  const pick = useCallback(
    () => onMessage(notice(t("qd picks the entry node itself."))),
    [onMessage],
  );
  const refresh = useCallback(async () => {
    await qd.refresh();
    onChanged();
  }, [onChanged]);

  if (status && !status.present) {
    return (
      <Card>
        <EmptyState
          icon={<Download />}
          title={t("qd is not downloaded yet")}
          hint={t(
            "It is fetched from the qd releases on GitHub and checked against the published checksum.",
          )}
          action={<Button onClick={onInstall}>{t("Download qd")}</Button>}
        />
      </Card>
    );
  }
  if (status && !status.elevated) {
    return (
      <Card>
        <EmptyState
          icon={<ShieldAlert />}
          title={t("qd needs administrator rights")}
          hint={t(
            "It captures traffic through WinDivert, which only works for an elevated process.",
          )}
          action={<Button onClick={onElevate}>{t("Restart as administrator")}</Button>}
        />
      </Card>
    );
  }

  const node = connected ? (state?.node ?? null) : null;
  const total = state?.nodes.total ?? 0;
  const face = node ?? (total === 1 ? (nodes[0] ?? null) : null);

  return (
    <div className="grid min-h-0 flex-1 grid-cols-[340px_minmax(0,1fr)] grid-rows-[minmax(0,1fr)] gap-3">
      <div className="flex min-h-0 flex-col gap-3 overflow-y-auto">
        {status?.problem && <Callout tone="danger" title={status.problem} />}
        {other && (
          <Callout
            tone="info"
            title={t("{engine} is running now", { engine: ENGINES[other].label })}
          >
            {t("Turning qd on stops {engine} first.", { engine: ENGINES[other].label })}
          </Callout>
        )}
        <Card padding="sm" aria-label={t("Connection")}>
          <div className="flex flex-col gap-3">
            <PowerRow
              on={connected}
              powering={powering}
              failed={Boolean(state?.failed)}
              headline={
                powering ? t("Connecting…") : connected ? t("Connected") : t("Disconnected")
              }
              detail={
                connected ? (
                  <>
                    qd · <Uptime started={started} fallback={t("just now")} />
                  </>
                ) : (
                  "qd"
                )
              }
              onPower={onPower}
            />

            <Item
              className="w-full"
              variant="surface"
              icon={<Server />}
              title={face ? face.name : t("No exit selected")}
              description={
                total > 1
                  ? node
                    ? t("won the race of {n} nodes", { n: total })
                    : t("the fastest of {n} nodes wins the race on connect", { n: total })
                  : undefined
              }
              meta={face && face.latencyMs !== undefined && <NodeDelay node={qd.asNode(face)} />}
            />

            <div className="flex w-full flex-col gap-2">
              {state?.allowExit && (
                <Field label={t("Exit")}>
                  <SegmentedControl
                    aria-label={t("Exit")}
                    fill
                    options={[
                      {
                        value: "entry" as const,
                        label: t("Entry node"),
                        hint: t("traffic leaves through the entry node"),
                      },
                      {
                        value: "egress" as const,
                        label: t("Exit node"),
                        hint: t("traffic leaves through the exit node"),
                      },
                    ]}
                    value={egress ? "egress" : "entry"}
                    onChange={(value) => flip({ egress: value === "egress" })}
                  />
                </Field>
              )}
              <Field label={t("Ads")}>
                <SegmentedControl
                  aria-label={t("Ads")}
                  fill
                  options={[
                    { value: "allow" as const, label: t("Allow") },
                    { value: "block" as const, label: t("Block") },
                  ]}
                  value={adblock ? "block" : "allow"}
                  disabled={!state}
                  onChange={(value) => flip({ adblock: value === "block" })}
                />
              </Field>
            </div>
          </div>
        </Card>
        <Speed
          running={connected}
          history={history}
          current={history.at(-1) ?? ZERO}
          totals={null}
        />
      </div>

      <Nodes
        nodes={shown}
        selected={nodes.find((item) => item.selected && connected)?.name ?? null}
        rules={false}
        sources={[]}
        hidden={false}
        rates={{}}
        onSelect={pick}
        onManual={() => {}}
        onChanged={reloadNodes}
        onAdd={onAdd}
        onMessage={onMessage}
        onRefresh={refresh}
        refreshLabel={t("Refresh the qd subscription")}
        emptyHint={t("Add the qd:// link in Sources.")}
        plain
      />
    </div>
  );
}
