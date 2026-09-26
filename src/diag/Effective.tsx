import { RefreshCw } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Card, CodeBlock, IconButton, Spinner } from "rootik";
import * as api from "../api";
import { t } from "../i18n";
import { failure, type Message } from "../shell/Banner";

/**
 * The assembled config (D-130): what the client hands the core, with everything the files
 * lack. Assembled on open and on a button, not by polling: text that redraws under the
 * reader's eyes cannot be read.
 */
export default function Effective({ onMessage }: { onMessage: (message: Message) => void }) {
  const [text, setText] = useState<string | null>(null);

  const load = useCallback(() => {
    api.configAssembled().then(setText, (e) => onMessage(failure(e)));
  }, [onMessage]);

  useEffect(load, [load]);

  return (
    <Card
      data-effective=""
      className="min-h-0 flex-1"
      title={t("Assembled config")}
      description={t(
        "What the core receives: your files plus sources, AUTO and umiray. Edited in the sections.",
      )}
      actions={
        <IconButton
          variant="ghost"
          icon={<RefreshCw />}
          label={t("Assemble again")}
          onClick={load}
        />
      }
    >
      {text === null ? (
        <Spinner label={t("Assembling")} />
      ) : (
        // Full card height: the code scrolls, not the column.
        <CodeBlock code={text} language="yaml" lineNumbers maxHeight="100%" className="h-full" />
      )}
    </Card>
  );
}
