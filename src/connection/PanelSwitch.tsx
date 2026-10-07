import { SegmentedControl } from "rootik";
import { t } from "../i18n";

/// Что показывает карточка справа в «Соединении» (D-160, D-172).
export type Panel = "nodes" | "groups" | "sources";

type Props = {
  value: Panel;
  onChange: (panel: Panel) => void;
  nodes: number;
  groups: number;
  sources: number;
};

/// Заголовок карточки справа: узлы, группы из них или откуда они. Счётчики — чтобы видеть
/// все три, не переключая.
export default function PanelSwitch({ value, onChange, nodes, groups, sources }: Props) {
  return (
    <SegmentedControl<Panel>
      size="sm"
      aria-label={t("Nodes, groups or sources")}
      value={value}
      onChange={onChange}
      options={[
        { value: "nodes", label: `${t("Nodes")} ${nodes}` },
        { value: "groups", label: `${t("Groups")} ${groups}` },
        { value: "sources", label: `${t("Sources")} ${sources}` },
      ]}
    />
  );
}
