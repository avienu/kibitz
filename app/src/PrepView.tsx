import { useEffect, useState } from "react";
import { matchingPlayers, prepView, type WeakLine } from "./lib/db";

interface PrepViewProps {
  /** Load a database game into the game view at the given ply. */
  onLoadGameAt: (gameId: number, ply: number) => void;
}

function fmtElo(elo: number | null): string {
  return elo === null ? "" : ` (${elo})`;
}

/**
 * Opponent prep (Phase 2): pick an opponent + color, rank their weakest
 * opening spots, and surface master games reaching those exact positions.
 * Uses the database opened in the Database tab.
 */
export default function PrepView({ onLoadGameAt }: PrepViewProps) {
  const [player, setPlayer] = useState("");
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [color, setColor] = useState<"white" | "black">("black");
  const [lines, setLines] = useState<WeakLine[] | null>(null);
  const [building, setBuilding] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Opponent-name suggestions (debounced substring match).
  useEffect(() => {
    const q = player.trim();
    if (q.length < 2) {
      setSuggestions([]);
      return;
    }
    let cancelled = false;
    const t = setTimeout(() => {
      matchingPlayers(q)
        .then((names) => !cancelled && setSuggestions(names))
        .catch(() => !cancelled && setSuggestions([]));
    }, 200);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [player]);

  const build = async () => {
    setBuilding(true);
    setError(null);
    setLines(null);
    try {
      setLines(await prepView(player.trim(), color));
    } catch (e) {
      setError(String(e));
    } finally {
      setBuilding(false);
    }
  };

  return (
    <div className="prepview">
      <div className="db-section">
        <h3>Opponent prep</h3>
        <div className="prep-form">
          <input
            type="text"
            list="prep-player-suggestions"
            value={player}
            onChange={(e) => setPlayer(e.target.value)}
            placeholder="opponent name (exact, suggestions as you type)…"
            spellCheck={false}
          />
          <datalist id="prep-player-suggestions">
            {suggestions.map((name) => (
              <option key={name} value={name} />
            ))}
          </datalist>
          <div className="prep-color">
            <button
              className={color === "white" ? "cur" : ""}
              onClick={() => setColor("white")}
              title="Prepare against their White repertoire"
            >
              as White
            </button>
            <button
              className={color === "black" ? "cur" : ""}
              onClick={() => setColor("black")}
              title="Prepare against their Black repertoire"
            >
              as Black
            </button>
          </div>
          <button onClick={() => void build()} disabled={building || player.trim() === ""}>
            {building ? "Building…" : "Build prep"}
          </button>
        </div>
        {error && (
          <div className="error">
            {error}
            {error.includes("no database open") && (
              <div className="hint">Open a database in the Database tab first.</div>
            )}
          </div>
        )}
        {lines && lines.length === 0 && (
          <div className="db-empty">
            No weak lines found for {player.trim()} as {color} (needs 3+ games per spot and an
            under-50% score).
          </div>
        )}
      </div>

      {lines &&
        lines.map((line, i) => (
          <div className="prep-card" key={line.hash}>
            <div className="prep-card-head">
              <span className="prep-rank">#{i + 1}</span>
              <span className="prep-weakness" title="Weakness score (higher = better prep target)">
                {line.weakness.toFixed(2)}
              </span>
              <span className="prep-meta">
                {line.games} games · scores {line.scorePct.toFixed(1)}% · reached by ply {line.ply}
              </span>
              {line.deviation && (
                <span className="prep-badge" title="A book-exit point of this opponent">
                  leaves book
                </span>
              )}
            </div>
            <div className="prep-moves">
              plays here: <span className="san">{line.opponentMoves.join(", ")}</span>
            </div>
            {line.masterGames.length > 0 ? (
              <div className="prep-masters">
                {line.masterGames.map((m) => (
                  <div
                    className="prep-master"
                    key={m.gameId}
                    title="Load this game at the prep position"
                    onClick={() => onLoadGameAt(m.gameId, m.ply)}
                  >
                    <span className="pm-players">
                      {m.white}
                      {fmtElo(m.whiteElo)} — {m.black}
                      {fmtElo(m.blackElo)}
                    </span>
                    <span className="pm-meta">
                      {m.event} · {m.date} · {m.result}
                    </span>
                  </div>
                ))}
              </div>
            ) : (
              <div className="db-empty">No master games reach this spot.</div>
            )}
          </div>
        ))}
    </div>
  );
}
