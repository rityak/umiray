import type * as api from "../api";
import type { Choices } from "../config/groups";
import { resolve } from "../config/groups";
import type { NodeSpeed } from "../hooks/useTraffic";
import { locale, t } from "../i18n";

/// Служебные имена групп клиента (D-113, D-172) — те же, что пишет сборка.
const AUTO = "AUTO";
/// Псевдоним выбора (D-053): на него смотрят `MATCH` и правила «через выбранный выход».
const ALIAS = "umiray";
const UDP = "umiray-udp";
const GEO = "umiray-geo-";
const PROTO = "umiray-proto-";

/// Группа так, как её показывает вкладка «Группы» (D-172): своя из документа групп или
/// собранная клиентом. Состав — имена узлов и групп.
export type GroupEntry = {
  name: string;
  kind: string;
  members: string[];
  /// Своя — её переименовывают; имена групп клиента держит сборка.
  own: boolean;
};

/// AUTO первым, за ним свои группы, потом остальные группы клиента: UDP и автогруппы.
export function entries(
  built: api.BuiltGroup[],
  user: api.Group[],
  choices: Choices,
): GroupEntry[] {
  const client = (group: api.BuiltGroup): GroupEntry => ({ ...group, own: false });
  return [
    ...built.filter((group) => group.name === AUTO).map(client),
    ...user.map((group) => ({
      name: group.name,
      kind: group.kind,
      // `proxies:` — чужие имена: другая группа, `DIRECT`. Их тоже видно в составе.
      members: [...resolve(group, choices), ...group.proxies],
      own: true,
    })),
    ...built.filter((group) => group.name !== AUTO).map(client),
  ];
}

/// Активная группа — первой: её раскрывают и смотрят, остальные ждут ниже в прежнем порядке.
export function activeFirst(list: GroupEntry[], active: string | null): GroupEntry[] {
  const at = list.findIndex((entry) => entry.name === active);
  return at <= 0 ? list : [list[at], ...list.slice(0, at), ...list.slice(at + 1)];
}

/// Название страны по коду на языке окна.
export function regionName(code: string): string {
  const upper = code.toUpperCase();
  try {
    return new Intl.DisplayNames([locale()], { type: "region" }).of(upper) ?? upper;
  } catch {
    return upper;
  }
}

/// Имя группы или цели для глаз — одно на всё окно: служебные имена сборки (`umiray`,
/// `umiray-geo-pl`, `umiray-proto-vless`) человек не видит, они остаются в конфиге ядра.
/// Страна — словами, протокол — как у узлов, псевдоним выбора — «Выбранный выход».
export function groupName(name: string, nodes: api.Node[] = []): string {
  if (name === ALIAS) return t("Selected exit");
  if (name.startsWith(GEO)) return regionName(name.slice(GEO.length));
  if (name.startsWith(PROTO)) {
    const kind = name.slice(PROTO.length);
    return (
      nodes.find((node) => node.kind.toLowerCase() === kind)?.kind ??
      kind.charAt(0).toUpperCase() + kind.slice(1)
    );
  }
  if (name === UDP) return t("UDP over UDP");
  return plainName(name);
}

/// Эмодзи, флаги и их склейки: флаг рисуется рядом картинкой (D-084), а прочее в имени
/// панели — украшение, которое Windows вдобавок рисует буквами.
const EMOJI = /\p{Extended_Pictographic}|\p{Regional_Indicator}|‍|️|⃣/gu;

/// Имя узла для глаз: без эмодзи и лишних пробелов. Имя целиком из эмодзи остаётся как есть —
/// пустая плитка хуже украшенной. Выбирают и пишут в конфиг по-прежнему исходное имя.
export function plainName(name: string): string {
  const plain = name.replace(EMOJI, " ").replace(/\s+/g, " ").trim();
  return plain === "" ? name : plain;
}

export function groupLabel(entry: GroupEntry, nodes: api.Node[]): string {
  return groupName(entry.name, nodes);
}

/// Значок группы: выбранный человеком, иначе — по смыслу группы клиента. `null` — свой
/// по типу (`GROUP_KIND`), его рисует вызывающий.
export function iconOf(entry: GroupEntry, icons: Record<string, string>): string | null {
  const chosen = icons[entry.name];
  if (chosen !== undefined) return chosen;
  if (entry.name === AUTO) return "lucide:Globe";
  if (entry.name === UDP) return "lucide:Zap";
  if (entry.name.startsWith(GEO)) return `flag:${entry.name.slice(GEO.length)}`;
  if (entry.name.startsWith(PROTO)) return "lucide:Layers";
  return null;
}

/// Кто из состава везёт трафик сейчас — самый загруженный — и сколько соединений у группы
/// всего (S-018). Узел без соединений не в счёт.
export function load(
  members: string[],
  rates: Record<string, NodeSpeed>,
): { lead: string | null; connections: number } {
  let lead: string | null = null;
  let best = -1;
  let connections = 0;
  for (const name of members) {
    const rate = rates[name];
    if (rate === undefined || rate.connections === 0) continue;
    connections += rate.connections;
    const weight = rate.down + rate.up + rate.connections;
    if (weight > best) {
      best = weight;
      lead = name;
    }
  }
  return { lead, connections };
}

/// Доля узла в нагрузке — ширина полосы под плиткой: от самого загруженного узла.
export function share(name: string, rates: Record<string, NodeSpeed>): number {
  const top = Math.max(0, ...Object.values(rates).map((rate) => rate.connections));
  const mine = rates[name]?.connections ?? 0;
  return top === 0 ? 0 : mine / top;
}

/// В AUTO ли узел при этих исключениях (D-172): его источник не вынут и он сам не вынут.
export function inAuto(node: api.Node, exclude: api.Exclude): boolean {
  return !exclude.sources.includes(node.source) && !exclude.nodes.includes(node.name);
}
