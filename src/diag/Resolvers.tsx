import { useEffect, useState } from "react";
import { Badge, Card, DataTable, Tooltip } from "rootik";
import * as api from "../api";
import { t, tn } from "../i18n";

type Row = {
  key: string;
  provider: string;
  note: string;
  variant: string;
  filter: string;
  proto: string;
  addr: string;
  ipv6: boolean;
};

/// The catalog flattened: people compare by column — protocol, filter — not by tree.
function flatten(providers: api.DnsProvider[]): Row[] {
  return providers.flatMap((provider) =>
    provider.variants.flatMap((variant) =>
      variant.servers.map((server) => ({
        key: `${provider.id}/${variant.id}/${server.addr}`,
        provider: provider.name,
        note: provider.note,
        variant: variant.name,
        filter: variant.filter,
        proto: server.proto,
        addr: server.addr,
        ipv6: server.ipv6,
      })),
    ),
  );
}

/**
 * Whom `dns-race` picks from (D-097): the whole `collections/dns.yaml` catalog.
 *
 * It sits next to the race because it answers the race's question "who was measured at all":
 * without it the line "94.140.14.14 won" does not say it is AdGuard with an ad filter. The
 * catalog is edited as a file (D-100) — here it is read only. Collapsed: needed when digging.
 */
export default function Resolvers() {
  const [rows, setRows] = useState<Row[] | null>(null);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!open || rows !== null) return;
    api.diagProviders().then(
      (catalog) => setRows(flatten(catalog.providers)),
      () => setRows([]),
    );
  }, [open, rows]);

  return (
    <Card
      padding="sm"
      collapsible
      open={open}
      onOpenChange={setOpen}
      title={t("Resolver catalog")}
      description={
        rows === null
          ? t("collections/dns.yaml — whom the race picks from")
          : tn(rows.length, "{n} address", "{n} addresses")
      }
    >
      {rows !== null && (
        <DataTable
          sticky
          maxHeight={360}
          density="compact"
          rows={rows}
          rowKey={(row) => row.key}
          columns={[
            {
              key: "provider",
              header: t("Provider"),
              sortable: true,
              value: (row) => row.provider,
              cell: (row) => (
                <Tooltip content={row.note}>
                  <span>{row.provider}</span>
                </Tooltip>
              ),
            },
            {
              key: "variant",
              header: t("Variant"),
              sortable: true,
              value: (row) => `${row.variant} · ${row.filter}`,
            },
            { key: "proto", header: t("Protocol"), sortable: true, value: (row) => row.proto },
            {
              key: "addr",
              header: t("Address"),
              mono: true,
              value: (row) => row.addr,
              cell: (row) => (
                <>
                  {row.addr}
                  {row.ipv6 && (
                    <Badge size="sm" variant="outline" className="ml-1.5">
                      IPv6
                    </Badge>
                  )}
                </>
              ),
            },
          ]}
        />
      )}
    </Card>
  );
}
