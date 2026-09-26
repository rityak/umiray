import type { Group, Node, Source } from "../api";

/**
 * Правило перевода «что отмечено мышью» ↔ «что записано в группе» (D-074).
 *
 * Узел поимённо не адресуется вовсе (S-012, перепроверено на D-122): всякий узел лежит
 * в провайдере, группа видит провайдер только через `use:`, а вырезать из него часть
 * можно единственным способом — `filter`. Своих имён в `proxies:` группы поэтому нет,
 * и всё, что там лежит, пришло не от нас: другая группа, `DIRECT`, `REJECT`.
 *
 * Отсюда два состояния выбора, и окно называет их словами:
 * - **живой список** — каждый затронутый источник взят целиком: в файле только `use:`,
 *   и новые узлы подписки приезжают в группу сами;
 * - **зафиксированный** — из источника взята часть: в файле `filter` с точными именами,
 *   и новые узлы сами не приедут.
 */
/// Из чего группе выбирать: все источники и все их узлы. Не коллекция (D-100) —
/// то поставляет клиент, а это приезжает от подписок.
export type Choices = {
  sources: Source[];
  nodes: Node[];
};

export type Selection = {
  /// Отмеченные узлы по именам. «Источник целиком» — это не отдельный режим, а «отмечены
  /// все его узлы»: так дерево остаётся одним списком, а не двумя видами выбора.
  picked: string[];
  /// Чем режется живой список. Пусто — берём всё.
  substring: string;
  /// Имена из `proxies:`, которых нет среди узлов: другая группа, `DIRECT`. Форма их
  /// показывает и сохраняет, но не правит — дерево про них ничего не знает.
  others: string[];
  /// Форма поняла группу целиком. Ложь — фильтр написан руками, и трогать его нельзя.
  understood: boolean;
};

/// Узлы источника, которые ядро возьмёт из провайдера, — они же всё, что можно отметить
/// в дереве. Неподдерживаемые не берём вовсе: до ядра они не доходят (D-063), и показывать
/// их выбираемыми значило бы врать.
export function nodesOf(choices: Choices, source: string): Node[] {
  return choices.nodes.filter((node) => node.source === source && node.supported);
}

/// Спецсимволы регулярного выражения. Имя узла попадает в фильтр как есть, и `Poland (1)`
/// без экранирования стало бы скобочной группой.
function quote(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/// Наш собственный фильтр «ровно эти имена» — и только он.
function exactNames(filter: string): string[] | null {
  const match = /^\^\((.*)\)\$$/.exec(filter);
  if (!match) return null;
  const names = match[1].split("|").map((part) => part.replace(/\\(.)/g, "$1"));
  // Сверяем обратной сборкой: понимаем только то, что записали сами. Чужой регексп
  // с той же формой мы бы иначе молча переписали.
  return exactFilter(names) === filter ? names : null;
}

function exactFilter(names: string[]): string {
  return `^(${names.map(quote).join("|")})$`;
}

/// Регистронезависимая подстрока. `(?i)` — форма записи RE2, которую понимает ядро;
/// в JavaScript такого синтаксиса нет, поэтому при показе он снимается флагом.
const NOCASE = "(?i)";

function plainSubstring(filter: string): string | null {
  const body = filter.startsWith(NOCASE) ? filter.slice(NOCASE.length) : filter;
  const plain = body.replace(/\\(.)/g, "$1");
  // Та же сверка обратной сборкой: подстрока — это то, что мы бы записали сами.
  // Иначе точка в `ru\.example` читалась бы как «любой символ», а форма показывала бы её
  // обычной буквой.
  return quote(plain) === body ? plain : null;
}

/// Что отмечено в дереве, если смотреть на уже записанную группу.
export function read(group: Group, choices: Choices): Selection {
  // Всё, что записано в `proxies:`, — чужое: своих имён мы туда не пишем. Форма их
  // показывает и сохраняет, но не правит.
  const others = group.proxies;
  const fromSources = group.sources.flatMap((id) => nodesOf(choices, id).map((n) => n.name));

  const filter = (group.filter ?? "").trim();
  if (filter === "") {
    return { picked: fromSources, substring: "", others, understood: true };
  }
  const exact = exactNames(filter);
  if (exact !== null) {
    const known = new Set(exact);
    return {
      picked: fromSources.filter((name) => known.has(name)),
      substring: "",
      others,
      understood: true,
    };
  }
  const substring = plainSubstring(filter);
  if (substring !== null) {
    return { picked: fromSources, substring, others, understood: true };
  }
  // Фильтр написан руками: разобрать его в галочки нельзя, а переписать — тем более.
  return { picked: fromSources, substring: "", others, understood: false };
}

/// Взят ли источник целиком. Пустой источник целиком не берётся: брать нечего.
export function whole(choices: Choices, source: string, picked: Set<string>): boolean {
  const nodes = nodesOf(choices, source);
  return nodes.length > 0 && nodes.every((node) => picked.has(node.name));
}

/// Источники, из которых что-то отмечено.
function involved(choices: Choices, picked: Set<string>): string[] {
  return choices.sources
    .map((source) => source.id)
    .filter((id) => nodesOf(choices, id).some((node) => picked.has(node.name)));
}

/// Живой ли список: каждый затронутый источник взят целиком. Только тогда осмысленна
/// подстрока — она продолжает действовать и на узлы, которых ещё нет.
export function live(choices: Choices, picked: Set<string>): boolean {
  return involved(choices, picked).every((id) => whole(choices, id, picked));
}

/// Записать выбор в группу. Поля, которых выбор не касается, остаются как были.
export function write(group: Group, selection: Selection, choices: Choices): Group {
  const picked = new Set(selection.picked);
  const sources = involved(choices, picked);
  const substring = selection.substring.trim();

  let filter: string | null = null;
  if (live(choices, picked)) {
    if (substring !== "") filter = `${NOCASE}${quote(substring)}`;
  } else {
    // Часть источника выбрана поимённо — значит списку придётся замереть: другого способа
    // вырезать узлы из провайдера у ядра нет.
    const exact = sources.flatMap((id) =>
      nodesOf(choices, id)
        .map((node) => node.name)
        .filter((name) => picked.has(name)),
    );
    filter = exactFilter(exact);
  }

  return { ...group, sources, proxies: selection.others, filter };
}

/// Кто окажется в группе на самом деле — по тем же правилам, по каким её прочитает ядро.
export function resolve(group: Group, choices: Choices): string[] {
  // Начинаем с пустого: в `proxies:` лежат только чужие имена — другая группа, `DIRECT`.
  // Их состав знает ядро, а не мы.
  const inside: string[] = [];
  const filter = (group.filter ?? "").trim();
  let matches: (name: string) => boolean = () => true;
  if (filter !== "") {
    try {
      const nocase = filter.startsWith(NOCASE);
      const pattern = new RegExp(nocase ? filter.slice(NOCASE.length) : filter, nocase ? "i" : "");
      matches = (name) => pattern.test(name);
    } catch {
      // Непонятный регексп — ядро такую группу тоже не соберёт; показываем пустой состав.
      matches = () => false;
    }
  }
  for (const id of group.sources) {
    for (const node of nodesOf(choices, id)) {
      if (matches(node.name)) inside.push(node.name);
    }
  }
  return inside;
}

/// Предпросмотр: отмеченные узлы и те из них, кого срезал фильтр.
export function preview(
  group: Group,
  selection: Selection,
  choices: Choices,
): { name: string; out: boolean }[] {
  const inside = new Set(resolve(write(group, selection, choices), choices));
  return selection.picked.map((name) => ({ name, out: !inside.has(name) }));
}
