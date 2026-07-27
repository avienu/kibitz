/**
 * Import PGN / SCID (DATA IN/OUT): paste or open a PGN to load it as the
 * active game. SCID (.si4) databases are converted with the CLI for now;
 * this screen documents that path.
 */
import { useRef, useState } from "react";

const SAMPLE_PGN = `[Event "London"]
[Site "London ENG"]
[Date "1851.06.21"]
[White "Adolf Anderssen"]
[Black "Lionel Kieseritzky"]
[Result "1-0"]

1. e4 e5 2. f4 exf4 3. Bc4 Qh4+ 4. Kf1 b5 5. Bxb5 Nf6 6. Nf3 Qh6
7. d3 Nh5 8. Nh4 Qg5 9. Nf5 c6 10. g4 Nf6 11. Rg1 cxb5 12. h4 Qg6
13. h5 Qg5 14. Qf3 Ng8 15. Bxf4 Qf6 16. Nc3 Bc5 17. Nd5 Qxb2
18. Bd6 Bxg1 19. e5 Qxa1+ 20. Ke2 Na6 21. Nxg7+ Kd8 22. Qf6+ Nxf6
23. Be7# 1-0`;

interface ImportViewProps {
  /** Parse + install the PGN as the active game (App owns the model). */
  onLoad: (pgnText: string) => void;
  status: string;
}

export default function ImportView({ onLoad, status }: ImportViewProps) {
  const [pgnText, setPgnText] = useState("");
  const fileRef = useRef<HTMLInputElement | null>(null);

  return (
    <div className="page import-view">
      <h2 className="page-title">Import PGN / SCID</h2>
      <div className="panel-box">
        <div className="panel-box-title">PGN</div>
        <textarea
          className="pgn-input"
          value={pgnText}
          onChange={(e) => setPgnText(e.target.value)}
          placeholder="Paste PGN here…"
          spellCheck={false}
        />
        <div className="row-gap">
          <button className="btn" onClick={() => onLoad(pgnText)}>
            Load
          </button>
          <button className="btn" onClick={() => fileRef.current?.click()}>
            Open file…
          </button>
          <button
            className="btn"
            onClick={() => {
              setPgnText(SAMPLE_PGN);
              onLoad(SAMPLE_PGN);
            }}
          >
            Sample game
          </button>
          <input
            ref={fileRef}
            type="file"
            accept=".pgn,.txt"
            style={{ display: "none" }}
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f) {
                void f.text().then((text) => {
                  setPgnText(text);
                  onLoad(text);
                });
              }
              e.target.value = "";
            }}
          />
        </div>
        <div className="settings-note">{status}</div>
      </div>
      <div className="panel-box">
        <div className="panel-box-title">SCID (.si4)</div>
        <p className="page-prose">
          SCID databases import through the command line for now:{" "}
          <code>kibitz-cli import-si4 &lt;base&gt;</code> converts a .si4/.sg4/.sn4 base into the
          SQLite database this app opens (Database in the rail). Legacy engine analysis is
          preserved and tagged — it is never deleted, only superseded by fresh analysis.
        </p>
      </div>
    </div>
  );
}
