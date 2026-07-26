import { useEffect, useState } from "react";
import {
  buildProfile,
  matchingPlayers,
  type MotifRow,
  type PhaseAcpl,
  type PlayerProfile,
} from "./lib/db";

interface ProfileViewProps {
  /** The last built profile (held by the parent so it survives tab switches). */
  profile: PlayerProfile | null;
  onProfileBuilt: (p: PlayerProfile) => void;
  /** Drill-down: load a database game at the given ply. */
  onLoadGameAt: (gameId: number, ply: number) => void;
}

function ExampleLinks({ ids, onLoadGameAt }: { ids: number[]; onLoadGameAt: ProfileViewProps["onLoadGameAt"] }) {
  if (ids.length === 0) return null;
  return (
    <>
      {ids.map((id) => (
        <button
          key={id}
          className="pf-ex"
          title={`Load game #${id} from the Database view`}
          onClick={() => onLoadGameAt(id, 1)}
        >
          #{id}
        </button>
      ))}
    </>
  );
}

function AcplRow({ label, a }: { label: string; a: PhaseAcpl }) {
  return (
    <tr>
      <td>{label}</td>
      <td className="num">{a.moves}</td>
      <td className="num">{a.moves > 0 ? a.acpl.toFixed(1) : "—"}</td>
      <td className="num">{a.blunders}</td>
      <td className="num">{a.mistakes}</td>
      <td className="num">{a.inaccuracies}</td>
    </tr>
  );
}

function motifTotal(m: MotifRow): number {
  return m.opportunities + m.taken + m.missed + m.allowed;
}

/**
 * Player profile report (run-4 goal 4): score & eval coverage, ACPL per
 * phase, motif matrix, structure and ECO tables — every example game id
 * clickable for drill-down — and the conversion/defense line.
 */
export default function ProfileView({ profile, onProfileBuilt, onLoadGameAt }: ProfileViewProps) {
  const [player, setPlayer] = useState(profile?.player ?? "");
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [building, setBuilding] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Player-name suggestions (debounced substring match, as in Prep).
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
    try {
      onProfileBuilt(await buildProfile(player.trim()));
    } catch (e) {
      setError(String(e));
    } finally {
      setBuilding(false);
    }
  };

  const p = profile;
  const conv = p?.conversion;

  return (
    <div className="profileview">
      <div className="db-section">
        <h3>Player profile</h3>
        <div className="prep-form">
          <input
            type="text"
            list="profile-player-suggestions"
            value={player}
            onChange={(e) => setPlayer(e.target.value)}
            placeholder="player name (exact, suggestions as you type)…"
            spellCheck={false}
          />
          <datalist id="profile-player-suggestions">
            {suggestions.map((name) => (
              <option key={name} value={name} />
            ))}
          </datalist>
          <button onClick={() => void build()} disabled={building || player.trim() === ""}>
            {building ? "Building…" : "Build profile"}
          </button>
        </div>
        {error && (
          <div className="error">
            {error}
            {error.includes("no database open") && (
              <div className="hint">Open a database from the Database rail item first.</div>
            )}
          </div>
        )}
        {p && (
          <div className="profile-summary">
            <strong>{p.player}</strong>: {p.games} games, scores {p.score_pct.toFixed(1)}% · engine
            eval coverage {p.eval_coverage_pct.toFixed(1)}% of their moves
          </div>
        )}
      </div>

      {p && (
        <div className="db-section">
          <h3>Accuracy by phase (ACPL)</h3>
          <table className="games-table">
            <thead>
              <tr>
                <th>Phase</th>
                <th className="num">Moves</th>
                <th className="num">ACPL</th>
                <th className="num">Blunders</th>
                <th className="num">Mistakes</th>
                <th className="num">Inaccuracies</th>
              </tr>
            </thead>
            <tbody>
              <AcplRow label="Opening" a={p.acpl_opening} />
              <AcplRow label="Middlegame" a={p.acpl_middlegame} />
              <AcplRow label="Endgame" a={p.acpl_endgame} />
            </tbody>
          </table>
        </div>
      )}

      {p && (
        <div className="db-section">
          <h3>Motif matrix</h3>
          {p.motifs.filter((m) => motifTotal(m) > 0).length === 0 ? (
            <div className="db-empty">No motif data (no medium+ alerts in the scanned games).</div>
          ) : (
            <table className="games-table">
              <thead>
                <tr>
                  <th>Motif</th>
                  <th className="num">Opportunities</th>
                  <th className="num">Taken</th>
                  <th className="num">Missed</th>
                  <th className="num">Allowed</th>
                  <th>Examples (missed / allowed)</th>
                </tr>
              </thead>
              <tbody>
                {p.motifs
                  .filter((m) => motifTotal(m) > 0)
                  .map((m) => (
                    <tr key={m.kind}>
                      <td>{m.kind}</td>
                      <td className="num">{m.opportunities}</td>
                      <td className="num">{m.taken}</td>
                      <td className="num">{m.missed}</td>
                      <td className="num">{m.allowed}</td>
                      <td>
                        <ExampleLinks ids={m.example_missed} onLoadGameAt={onLoadGameAt} />
                        {m.example_missed.length > 0 && m.example_allowed.length > 0 && " / "}
                        <ExampleLinks ids={m.example_allowed} onLoadGameAt={onLoadGameAt} />
                      </td>
                    </tr>
                  ))}
              </tbody>
            </table>
          )}
        </div>
      )}

      {p && (
        <div className="db-section">
          <h3>Pawn structures &amp; piece placement</h3>
          {p.structures.length === 0 ? (
            <div className="db-empty">No recurring structure flags found.</div>
          ) : (
            <table className="games-table">
              <thead>
                <tr>
                  <th>Structure</th>
                  <th className="num">Games</th>
                  <th className="num">Score %</th>
                  <th>Examples</th>
                </tr>
              </thead>
              <tbody>
                {p.structures.map((s) => (
                  <tr key={s.flag}>
                    <td>{s.flag}</td>
                    <td className="num">{s.games}</td>
                    <td className="num">{s.score_pct.toFixed(1)}</td>
                    <td>
                      <ExampleLinks ids={s.examples} onLoadGameAt={onLoadGameAt} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      )}

      {p && (
        <div className="db-section">
          <h3>Openings (ECO)</h3>
          {p.eco.length === 0 ? (
            <div className="db-empty">No ECO data.</div>
          ) : (
            <table className="games-table">
              <thead>
                <tr>
                  <th>ECO</th>
                  <th className="num">Games</th>
                  <th className="num">Score %</th>
                  <th>Examples</th>
                </tr>
              </thead>
              <tbody>
                {p.eco.map((e) => (
                  <tr key={e.eco}>
                    <td>{e.eco}</td>
                    <td className="num">{e.games}</td>
                    <td className="num">{e.score_pct.toFixed(1)}</td>
                    <td>
                      <ExampleLinks ids={e.examples} onLoadGameAt={onLoadGameAt} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      )}

      {p && conv && (
        <div className="db-section">
          <h3>Conversion &amp; defense</h3>
          <div className="profile-line">
            Converted {conv.converted_wins} of {conv.winning_reached} winning positions (≥ +2.00);
            held {conv.held} of {conv.losing_reached} worse positions (≤ −1.00).
            {p.eval_coverage_pct === 0 && (
              <span className="hint">
                {" "}
                No engine evals stored — run Re-analyze + Run jobs on games first.
              </span>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
