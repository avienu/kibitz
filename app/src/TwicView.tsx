/**
 * TWIC ingest screen (run 9 — the maintainer's "I need to see every single
 * week" ruling): the full issue catalog from the earliest zip the archive
 * serves through the latest known issue, with per-issue import status,
 * multi-select download, "Download all missing", an auto-download toggle,
 * and the inline-job-row progress grammar with cooperative cancel.
 *
 * Honesty rules: dates are labelled "approx" (weekly arithmetic, not
 * scraped); the published frontier is only refreshed on the explicit
 * "Refresh catalog" action (a handful of HEAD requests, count shown);
 * the kibitz-db FIRST_RUN_NOTICE must be acknowledged before the first
 * download; the personal-use line stays in the footer. TWIC data is never
 * bundled or redistributed — downloads go to the user's own database.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import DataTable, { type DataTableColumn } from "./components/DataTable";
import ScreenHeader from "./shell/ScreenHeader";
import {
  missingIssues,
  netCancel,
  twicAckNotice,
  twicCatalog,
  twicDownload,
  twicRefreshCatalog,
  twicSetAutoSync,
  type NetProgress,
  type TwicCatalog,
  type TwicCatalogRow,
} from "./lib/net";

const PAGE_SIZE = 100;

const GRID = "40px 90px 150px 1fr 90px";

interface TwicViewProps {
  /** App-level poll of the shared network worker (netops.rs). */
  progress: NetProgress | null;
}

export default function TwicView({ progress }: TwicViewProps) {
  const [catalog, setCatalog] = useState<TwicCatalog | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [page, setPage] = useState(0);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [refreshing, setRefreshing] = useState(false);
  const [refreshNote, setRefreshNote] = useState<string | null>(null);
  const [notice, setNotice] = useState<number[] | null>(null); // pending issues
  const [note, setNote] = useState<string | null>(null);
  const [cancelling, setCancelling] = useState(false);

  const loadCatalog = useCallback(() => {
    twicCatalog()
      .then((c) => {
        setCatalog(c);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  }, []);

  useEffect(loadCatalog, [loadCatalog]);

  // When the shared worker finishes a TWIC job, the catalog rows changed.
  const wasActive = useRef(false);
  useEffect(() => {
    const active = progress?.active ?? false;
    if (wasActive.current && !active) {
      loadCatalog();
      setCancelling(false);
    }
    wasActive.current = active;
  }, [progress, loadCatalog]);

  const twicJob =
    progress && (progress.kind === "twic" || progress.kind === "twic-auto") ? progress : null;
  const busy = progress?.active ?? false;

  /* ---- download plumbing ---- */

  const startDownload = useCallback(
    async (issues: number[]) => {
      setNote(null);
      try {
        const n = await twicDownload(issues);
        setNote(`${n} issue${n === 1 ? "" : "s"} queued — downloading serially.`);
        setSelected(new Set());
      } catch (e) {
        setNote(String(e));
      }
    },
    [],
  );

  const requestDownload = useCallback(
    (issues: number[]) => {
      if (issues.length === 0 || !catalog) return;
      // The kibitz-db first-run notice: shown (and acknowledged) in-UI
      // before the very first download into an empty twic_issues table.
      if (catalog.latestImported === null && !catalog.noticeAcknowledged) {
        setNotice(issues);
        return;
      }
      void startDownload(issues);
    },
    [catalog, startDownload],
  );

  const acknowledgeAndStart = useCallback(async () => {
    if (!notice) return;
    const issues = notice;
    setNotice(null);
    try {
      await twicAckNotice();
      setCatalog((c) => (c ? { ...c, noticeAcknowledged: true } : c));
      await startDownload(issues);
    } catch (e) {
      setNote(String(e));
    }
  }, [notice, startDownload]);

  const doRefresh = useCallback(async () => {
    setRefreshing(true);
    setRefreshNote(null);
    try {
      const r = await twicRefreshCatalog();
      setRefreshNote(
        r.latestKnown !== null
          ? `Latest published: TWIC ${r.latestKnown} (checked with ${r.requests} HEAD request${r.requests === 1 ? "" : "s"}).`
          : `No published issue found (${r.requests} HEAD request${r.requests === 1 ? "" : "s"}).`,
      );
      loadCatalog();
    } catch (e) {
      setRefreshNote(`Refresh failed: ${e}`);
    } finally {
      setRefreshing(false);
    }
  }, [loadCatalog]);

  const doCancel = useCallback(async () => {
    setCancelling(true);
    try {
      await netCancel();
      setNote("Cancelling after the current issue — imported issues stay imported.");
    } catch (e) {
      setNote(String(e));
      setCancelling(false);
    }
  }, []);

  const toggleAuto = useCallback(async () => {
    if (!catalog) return;
    const next = !catalog.autoSync;
    try {
      await twicSetAutoSync(next);
      setCatalog((c) => (c ? { ...c, autoSync: next } : c));
    } catch (e) {
      setNote(String(e));
    }
  }, [catalog]);

  /* ---- table model ---- */

  const rows = catalog?.rows ?? [];
  const missing = missingIssues(rows);
  const totalPages = Math.max(1, Math.ceil(rows.length / PAGE_SIZE));
  const pageRows = rows.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE);
  const pageMissing = missingIssues(pageRows);
  const importedCount = rows.length - missing.length;

  const toggleIssue = useCallback((issue: number) => {
    setSelected((s) => {
      const next = new Set(s);
      if (next.has(issue)) next.delete(issue);
      else next.add(issue);
      return next;
    });
  }, []);

  const columns: DataTableColumn<TwicCatalogRow>[] = [
    {
      key: "sel",
      header: "",
      render: (r) =>
        r.imported ? null : (
          <input
            type="checkbox"
            className="twic-check"
            checked={selected.has(r.issue)}
            onChange={() => toggleIssue(r.issue)}
            aria-label={`Select TWIC ${r.issue}`}
          />
        ),
    },
    {
      key: "issue",
      header: "ISSUE",
      render: (r) => <span className="cell-eco">{r.issue}</span>,
      sort: (a, b) => a.issue - b.issue,
    },
    {
      key: "week",
      header: "WEEK (APPROX)",
      render: (r) => <span className="cell-date">≈ {r.approxDate}</span>,
    },
    {
      key: "status",
      header: "STATUS",
      render: (r) =>
        r.imported ? (
          <span className="twic-imported">imported</span>
        ) : (
          <span className="twic-missing">not downloaded</span>
        ),
      sort: (a, b) => Number(a.imported) - Number(b.imported),
    },
    {
      key: "games",
      header: "GAMES",
      align: "right",
      render: (r) => (r.games !== null ? r.games.toLocaleString("en-US") : "—"),
    },
  ];

  const subtitle = catalog
    ? catalog.latestKnown !== null
      ? `${importedCount.toLocaleString("en-US")} of ${rows.length.toLocaleString("en-US")} issues imported · ` +
        `TWIC ${catalog.firstAvailable}–${catalog.latestKnown}` +
        (catalog.latestImported !== null ? ` · newest imported wk ${catalog.latestImported}` : "")
      : "catalog not discovered yet"
    : error
      ? "no database open"
      : "loading…";

  return (
    <>
      <ScreenHeader
        title="TWIC ingest"
        subtitle={subtitle}
        actions={
          catalog && (
            <>
              <button className="btn-secondary" onClick={() => void doRefresh()} disabled={refreshing || busy}>
                {refreshing ? "Checking…" : "Refresh catalog"}
              </button>
              <button
                className="btn-secondary"
                onClick={() => requestDownload(missing)}
                disabled={busy || missing.length === 0}
              >
                Download all missing ({missing.length.toLocaleString("en-US")})
              </button>
            </>
          )
        }
      />
      <div className="page-scroll">
        <div className="twic-page">
          {error && (
            <div className="error">
              {error} — open a database on the Database screen first.
            </div>
          )}
          {refreshNote && <div className="twic-note">{refreshNote}</div>}
          {note && <div className="twic-note">{note}</div>}

          {catalog && (
            <label className="twic-auto">
              <input type="checkbox" checked={catalog.autoSync} onChange={() => void toggleAuto()} />
              Automatically download new issues when the database opens — newest issues first,
              max 5 per app launch (enforced even if the window reloads); older issues stay
              manual via Download all missing. Strictly serial; also in Settings → Data.
            </label>
          )}

          {twicJob && (twicJob.active || twicJob.error) && (
            <div className="inline-job-row">
              <span className="inline-job-label">
                {twicJob.kind === "twic-auto" ? "TWIC AUTO-SYNC" : "DOWNLOADING TWIC"}
              </span>
              <span className="inline-job-track">
                <span
                  className="inline-job-fill"
                  style={{
                    width: `${twicJob.total > 0 ? Math.round((twicJob.done / twicJob.total) * 100) : 0}%`,
                  }}
                />
              </span>
              <span className="inline-job-detail">
                {twicJob.error
                  ? `failed: ${twicJob.error}`
                  : `${twicJob.done} / ${twicJob.total} · ${twicJob.detail}`}
              </span>
              {twicJob.active && (
                <button className="btn-ghost" onClick={() => void doCancel()} disabled={cancelling}>
                  {cancelling ? "Cancelling…" : "Cancel"}
                </button>
              )}
            </div>
          )}

          {catalog && catalog.latestKnown === null && (
            <div className="panel-box">
              <p className="page-prose">
                Nothing is known about the published catalog yet. <strong>Refresh catalog</strong>{" "}
                asks theweekinchess.com which issue is newest — a handful of HEAD requests
                (typically 2, at most 12), only ever on this explicit action. The archive serves
                downloadable issues from TWIC {catalog.firstAvailable} (≈ 2012) onward.
              </p>
            </div>
          )}

          {catalog && rows.length > 0 && (
            <>
              <div className="twic-toolbar">
                <button
                  className="btn-ghost"
                  onClick={() =>
                    setSelected((s) => new Set([...s, ...pageMissing]))
                  }
                  disabled={busy || pageMissing.length === 0}
                >
                  Select page
                </button>
                <button
                  className="btn-ghost"
                  onClick={() => setSelected(new Set())}
                  disabled={selected.size === 0}
                >
                  Clear selection
                </button>
                <button
                  className="btn-secondary"
                  onClick={() => requestDownload([...selected])}
                  disabled={busy || selected.size === 0}
                >
                  Download selected ({selected.size})
                </button>
              </div>

              <DataTable
                columns={columns}
                rows={pageRows}
                gridTemplate={GRID}
                rowKey={(r) => r.issue}
                rowClassName={(r) => (r.imported ? "twic-row-imported" : undefined)}
                empty="No issues known."
                footer={
                  <div className="pager-row">
                    <button
                      className="btn-ghost"
                      onClick={() => setPage((p) => Math.max(0, p - 1))}
                      disabled={page === 0}
                    >
                      ◀
                    </button>
                    <span className="pager-readout">
                      page {page + 1} of {totalPages}
                    </span>
                    <button
                      className="btn-ghost"
                      onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
                      disabled={page + 1 >= totalPages}
                    >
                      ▶
                    </button>
                    <div className="filter-spacer" />
                    <span className="pager-note">
                      Week dates are approximate (weekly arithmetic from TWIC 1000 =
                      2014-01-06); issues occasionally slip a few days.
                    </span>
                  </div>
                }
              />
            </>
          )}

          <p className="twic-footnote">
            The Week in Chess is compiled by Mark Crowther and funded by reader donations.
            Downloads are strictly serial, an issue is never fetched twice, and the data is for
            your personal use only — Kibitz never bundles or redistributes TWIC data. Consider
            donating at https://theweekinchess.com/twic.
          </p>
        </div>
      </div>

      {notice && catalog && (
        <div className="modal-overlay" onClick={() => setNotice(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-title">Before your first TWIC download</div>
            <p className="modal-prose">{catalog.firstRunNotice}</p>
            <div className="modal-actions">
              <button className="btn-secondary" onClick={() => setNotice(null)}>
                Cancel
              </button>
              <button className="btn-primary" onClick={() => void acknowledgeAndStart()}>
                I understand — personal use only
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
