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
            {t("Not now")}
          </Button>
          <Button size="sm" variant="primary" onClick={onAccept}>
            {t("Create task")}
          </Button>
        </>
      }
    >
      {t(
        "You have admin rights now, so the scheduler task can be created right away. Then TUN starts at once, with no UAC prompt at launch or sign-in. Remove it in Settings → Umiray Settings.",
      )}
    </Callout>
  );
}
