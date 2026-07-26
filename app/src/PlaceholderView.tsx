/**
 * Placeholder panels for capabilities that exist in the data layer but
 * have no UI surface yet (TWIC ingest, account syncs). They reserve the
 * rail entry (the discoverability fix) and document the CLI path — no
 * fake status, no fake numbers.
 */

interface PlaceholderViewProps {
  title: string;
  children: React.ReactNode;
}

export default function PlaceholderView({ title, children }: PlaceholderViewProps) {
  return (
    <div className="page placeholder-view">
      <h2 className="page-title">{title}</h2>
      <div className="panel-box">{children}</div>
    </div>
  );
}

export function TwicPlaceholder() {
  return (
    <PlaceholderView title="TWIC ingest">
      <p className="page-prose">
        The Week in Chess ingester runs from the command line for now:{" "}
        <code>silman-cli twic-ingest</code> downloads new weekly issues into the open database
        (personal use only — TWIC data is never bundled or redistributed). The last ingested week
        will appear here, and in the rail badge, once the desktop surface lands.
      </p>
    </PlaceholderView>
  );
}

export function SyncsPlaceholder() {
  return (
    <PlaceholderView title="Account syncs">
      <p className="page-prose">
        Online-account game sync (Lichess, chess.com, FICS) lives in the data layer and the CLI:{" "}
        <code>silman-cli sync</code>. Connected accounts and their last sync will appear here once
        the desktop surface lands. Provenance of every imported game is tracked in the database.
      </p>
    </PlaceholderView>
  );
}
