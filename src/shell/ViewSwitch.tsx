import { Code2, FormInput, List } from "lucide-react";
import { SegmentedControl } from "rootik";
import { t } from "../i18n";

export type View = "visual" | "code";

/**
 * "Form / Code" — two spellings of one document (D-065, D-074). One switch for every
 * section, always first on the left of the bar (`SectionBar`): so it stays in one place
 * wherever you go. In Sources the form is a list.
 */
export default function ViewSwitch({
  value,
  onChange,
  list = false,
}: {
  value: View;
  onChange: (view: View) => void;
  list?: boolean;
}) {
  return (
    <SegmentedControl<View>
      aria-label={t("View")}
      value={value}
      onChange={onChange}
      options={[
        list
          ? { value: "visual", label: t("List"), icon: <List /> }
          : { value: "visual", label: t("Form"), icon: <FormInput /> },
        { value: "code", label: t("Code"), icon: <Code2 /> },
      ]}
    />
  );
}
