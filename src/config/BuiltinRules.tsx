import { ListChecks } from "lucide-react";
import { useEffect, useState } from "react";
import { Card, CodeBlock } from "rootik";
import * as api from "../api";
import { t } from "../i18n";

/**
 * Enabled built-in rule sets — where they land: between your rules and `MATCH` (D-083).
 * For reference only: they are not edited or switched off here — that is «Settings».
 */
export default function BuiltinRules() {
  const [sets, setSets] = useState<api.Ruleset[]>([]);

  useEffect(() => {
    api.rulesetsList().then(
      (list) => setSets(list.filter((set) => set.on)),
      () => setSets([]),
    );
  }, []);

  return (
    <>
      {sets.map((set) => (
        <Card
          key={set.id}
          variant="outline"
          padding="sm"
          collapsible
          defaultOpen={false}
          headingLevel={3}
          icon={<ListChecks />}
          title={set.title}
          description={t("built-in set · {n} lines", { n: set.rules.length })}
        >
          <CodeBlock code={set.rules.join("\n")} maxHeight={160} />
        </Card>
      ))}
    </>
  );
}
