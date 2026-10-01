import { Plus } from "lucide-react";
import { useEffect, useState } from "react";
import {
  Button,
  Dialog,
  EmptyState,
  Field,
  Input,
  SegmentedControl,
  Select,
  Spinner,
} from "rootik";
import * as api from "../api";
import { getLang, t, tk } from "../i18n";
import { failure } from "../shell/Banner";

type Props = {
  /// Add from the catalog or by address. A refusal is thrown and shown at the field;
  /// the caller closes on success.
  onCatalog: (id: string) => Promise<void>;
  onUrl: (url: string, title: string) => Promise<void>;
  onClose: () => void;
  /// Already in this route (D-158): offered, but not twice.
  taken: string[];
};

type Source = "catalog" | "url";

/// Catalog parts in the order they matter (`collections/lists.yaml`, D-157).
const GROUPS = [
  { id: "blocked", label: tk("Blocked in Russia") },
  { id: "services", label: tk("Services") },
  { id: "russia", label: tk("Russia — direct") },
];

/// Where a list comes from, without the path: the catalog file is edited by hand, and a
/// broken address must not take the dialog down.
function host(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

function localized(offer: api.ListOffer): { title: string; note: string } {
  const en = getLang() === "en";
  return {
    title: en ? (offer.titleEn ?? offer.title) : offer.title,
    note: en ? (offer.noteEn ?? offer.note) : offer.note,
  };
}

/**
 * Add a rule set (D-157): from the catalog or by address. Downloading and building a large
 * list takes seconds, so the button waits; a refusal shows at the field, success closes.
 */
export default function ListDialog({ onCatalog, onUrl, onClose, taken }: Props) {
  const [source, setSource] = useState<Source>("catalog");
  const [offers, setOffers] = useState<api.ListOffer[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [picked, setPicked] = useState<string | null>(null);
  const [url, setUrl] = useState("");
  const [title, setTitle] = useState("");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);

  // Окно монтируется заново при каждом открытии: занятое нужно один раз — для первого выбора.
  // biome-ignore lint/correctness/useExhaustiveDependencies: каталог читается раз за открытие
  useEffect(() => {
    api.listsCatalog().then(
      (list) => {
        setOffers(list);
        setPicked(list.find((offer) => !taken.includes(offer.id))?.id ?? null);
      },
      (e) => setLoadError(failure(e).text),
    );
  }, []);

  const ready = source === "catalog" ? picked !== null : url.trim() !== "";

  const add = async () => {
    setBusy(true);
    setFailed(null);
    try {
      if (source === "catalog" && picked) await onCatalog(picked);
      if (source === "url") await onUrl(url.trim(), title.trim());
    } catch (e) {
      setFailed(failure(e).text);
    } finally {
      setBusy(false);
    }
  };

  const groups = GROUPS.map((group) => ({
    label: t(group.label),
    options: (offers ?? [])
      .filter((offer) => offer.group === group.id)
      .map((offer) => {
        const { title, note } = localized(offer);
        return {
          value: offer.id,
          label: title,
          hint: taken.includes(offer.id) ? t("already added") : note,
          disabled: taken.includes(offer.id),
        };
      }),
  })).filter((group) => group.options.length > 0);
  // A group the catalog file invented is still shown — the file belongs to the user (D-100).
  const others = (offers ?? []).filter(
    (offer) => !GROUPS.some((group) => group.id === offer.group),
  );
  if (others.length > 0) {
    groups.push({
      label: t("Other"),
      options: others.map((offer) => ({
        value: offer.id,
        label: localized(offer).title,
        hint: taken.includes(offer.id) ? t("already added") : localized(offer).note,
        disabled: taken.includes(offer.id),
      })),
    });
  }
  const chosen = offers?.find((offer) => offer.id === picked);

  return (
    <Dialog
      open
      size="md"
      title={t("Rule set")}
      description={t("A list of domains and subnets. Use it in a rule as RULE-SET.")}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("Cancel")}
          </Button>
          <Button variant="primary" icon={<Plus />} loading={busy} disabled={!ready} onClick={add}>
            {t("Add")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <SegmentedControl
          aria-label={t("Where from")}
          fill
          options={[
            { value: "catalog", label: t("Catalog") },
            { value: "url", label: t("By address") },
          ]}
          value={source}
          onChange={(next) => {
            setSource(next as Source);
            setFailed(null);
          }}
        />
        {source === "catalog" ? (
          offers === null ? (
            loadError ? (
              <EmptyState tone="danger" title={t("The catalog did not open")} hint={loadError} />
            ) : (
              <Spinner label={t("Loading")} />
            )
          ) : (
            <Field
              label={t("List")}
              hint={
                chosen && !failed
                  ? [localized(chosen).note, ...new Set(chosen.urls.map(host))]
                      .filter(Boolean)
                      .join(" · ")
                  : undefined
              }
              error={failed ?? undefined}
            >
              <Select
                aria-label={t("List")}
                value={picked ?? undefined}
                placeholder={t("Everything is already added")}
                options={groups}
                onChange={(next) => setPicked(next)}
              />
            </Field>
          )
        ) : (
          <>
            <Field label={t("Address")} error={failed ?? undefined}>
              <Input
                type="url"
                mono
                value={url}
                placeholder="https://…/list.txt"
                spellCheck={false}
                onChange={(event) => setUrl(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && ready && !busy) add();
                }}
              />
            </Field>
            <Field label={t("Name")} hint={t("Empty — the file name from the address.")}>
              <Input value={title} onChange={(event) => setTitle(event.target.value)} />
            </Field>
          </>
        )}
      </div>
    </Dialog>
  );
}
