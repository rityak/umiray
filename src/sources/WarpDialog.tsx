import { Cloud } from "lucide-react";
import { useState } from "react";
import { Button, Callout, ChoiceCards, Dialog, Field } from "rootik";
import * as api from "../api";
import { t, tk } from "../i18n";
import { failure } from "../shell/Banner";

type Props = {
  /// The node is issued and added; the caller closes the dialog and says so.
  onDone: (result: api.Import) => void;
  onClose: () => void;
};

const TUNNELS: { value: api.WarpTunnel; label: string; description: string }[] = [
  {
    value: "masque",
    label: "MASQUE",
    description: tk("QUIC that looks like ordinary HTTP/3 — harder to single out"),
  },
  {
    value: "wireguard",
    label: "WireGuard",
    description: tk("classic WARP; the handshake is masked"),
  },
];

/**
 * Cloudflare WARP issued by the client itself (D-165): a new device at Cloudflare becomes a
 * node in "My nodes". Pressing "Issue" is the person's consent to Cloudflare's terms — the
 * dialog says so in words, since that is what registering a device means.
 */
export default function WarpDialog({ onDone, onClose }: Props) {
  const [tunnel, setTunnel] = useState<api.WarpTunnel>("masque");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);

  const issue = async () => {
    setBusy(true);
    setFailed(null);
    try {
      onDone(await api.sourcesAddWarp(tunnel));
    } catch (e) {
      setFailed(failure(e).text);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open
      size="sm"
      title="Cloudflare WARP"
      description={t(
        "A free Cloudflare node: the client registers a device and adds it to your nodes.",
      )}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("Cancel")}
          </Button>
          <Button variant="primary" icon={<Cloud />} loading={busy} onClick={issue}>
            {t("Issue")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <Field label={t("Tunnel")}>
          <ChoiceCards<api.WarpTunnel>
            aria-label={t("Tunnel")}
            minWidth={150}
            value={tunnel}
            onChange={setTunnel}
            options={TUNNELS.map((item) => ({
              value: item.value,
              label: item.label,
              description: t(item.description),
            }))}
          />
        </Field>
        <Callout
          tone="info"
          title={t("Issuing creates a WARP account at Cloudflare and accepts its terms")}
        >
          cloudflare.com/application/terms
        </Callout>
        {failed && <Callout tone="danger" title={failed} />}
      </div>
    </Dialog>
  );
}
