import { ShieldCheck } from "lucide-react";
import { Button, Callout } from "rootik";
import { t } from "../i18n";

type Props = {
  onAccept: () => void;
  onDismiss: () => void;
};

/**
 * An offer to pin administrator rights (D-087). Shows once: the client already runs
 * elevated, there is no task yet and it has not been declined. Buttons name the outcome.
 */
export default function AdminOffer({ onAccept, onDismiss }: Props) {
  return (
    <Callout
      tone="accent"
      icon={<ShieldCheck />}
      title={t("Always run umiray as administrator?")}
      actions={
        <>
          <Button size="sm" variant="ghost" onClick={onDismiss}>
            {t("Don't pin")}
          </Button>
          <Button size="sm" variant="primary" onClick={onAccept}>
            {t("Pin")}
          </Button>
        </>
      }
    >
      {t(
        "The rights are here now — the scheduler task can be created right from here. After that TUN comes up at once, and UAC asks neither at launch nor at sign-in. Remove it in Settings → Umiray Settings.",
      )}
    </Callout>
  );
}
