import { SegmentedControl } from "rootik";
import { t } from "../i18n";

/// Что показывает карточка справа в «Соединении» (D-160).
export type Panel = "nodes" | "sources";

type Props = {
  value: Panel;
  onChange: (panel: Panel) => void;
  nodes: number;
  sources: number;
};

/// Заголовок карточки справа: узлы или откуда они. Счётчики — чтобы видеть оба, не переключая.
export default function PanelSwitch({ value, onChange, nodes, sources }: Props) {
  return (
    <SegmentedControl<Panel>
      size="sm"
      aria-label={t("Nodes or sources")}
      value={value}
      onChange={onChange}
      options={[
        { value: "nodes", label: `${t("Nodes")} ${nodes}` },
        { value: "sources", label: `${t("Sources")} ${sources}` },
      ]}
    />
  );
}
