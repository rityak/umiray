import { Badge, DataTable } from "rootik";
import type * as api from "../api";
import { t } from "../i18n";
import { measure } from "./measure";
import Verdict from "./Verdict";

type Row = api.DiagRow & { at: number };

/// A report table: headers and rows arrive ready from the utility (D-097). Counting and
/// formatting are its job; the window only shows.
export default function ReportTable({ report }: { report: api.Report }) {
  if (report.rows.length === 0) return null;
  const rows: Row[] = report.rows.map((row, at) => ({ ...row, at }));
  // A column of quantities sorts as numbers when every non-empty cell reads as one:
  // otherwise "1.1 s" would sort before "235 ms".
  const numeric = report.columns.map((_, index) =>
    rows.every((row) => !row.cells[index] || measure(row.cells[index]) !== null),
  );
  return (
    <DataTable
      density="compact"
      rows={rows}
      // Two resolvers of one provider share both name and variant: the key is the position.
      rowKey={(row) => String(row.at)}
      columns={[
        ...report.columns.map((header, index) => ({
          key: `c${index}`,
          header,
          mono: true,
          sortable: true,
          value: (row: Row) =>
            numeric[index]
              ? (measure(row.cells[index] ?? "") ?? Number.POSITIVE_INFINITY)
              : (row.cells[index] ?? ""),
          cell: (row: Row) => row.cells[index] ?? "",
        })),
        {
          key: "verdict",
          header: t("Verdict"),
          cell: (row: Row) => (
            <span className="inline-flex items-center gap-1.5">
              <Verdict value={row.verdict} />
              {/* What the utility marked is what it suggests taking. */}
              {row.mark && (
                <Badge size="sm" tone="accent">
                  {t("take")}
                </Badge>
              )}
            </span>
          ),
        },
      ]}
    />
  );
}
