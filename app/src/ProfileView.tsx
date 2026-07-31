/**
 * Profile — round-2 build-out (design/handoff-2 §Screen: Profile).
 *
 * One screen, two subjects: the header segmented control switches between
 * You and the opponent carried in by navigation. Content (serif lede,
 * motif matrix, structure report, phase accuracy, conversion & defence)
 * beside the 420px evidence aside. EVERY NUMBER IS A CONTROL: clicking a
 * motif row/cell, structure bar, phase tile or rate tile re-targets the
 * aside; an evidence row opens the game view at the ply that produced the
 * claim. The `claim` prop (Home's findings rows) pre-selects.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import BaselineBar from "./components/BaselineBar";
import DataTable, { type DataTableColumn } from "./components/DataTable";
import EvidencePane, { type EvidenceGame } from "./components/EvidencePane";
import StatTile from "./components/StatTile";
import ScreenHeader from "./shell/ScreenHeader";
import {
  aliasDeclare,
  aliasRemove,
  buildProfile,
  identityGroup,
  cacheProfile,
  selfPlayerGet,
  selfPlayerSet,
  getGame,
  matchingPlayers,
  type MotifRow,
  type NameForm,
  type PlayerProfile,
  type ProfileExample,
} from "./lib/db";
import {
  claimId,
  claimTarget,
  parseClaim,
  phaseNote,
  profileLede,
  rankedMotifs,
  ratePct,
  sameClaim,
  shortMotif,
  trainableMotif,
  type Claim,
} from "./lib/profileView";
import type { ViewId, ViewParams } from "./lib/shell";

interface ProfileViewProps {
  /** Deep-link auto-build: set the self player and build on mount. */
  initialPlayer?: string | null;
  /** The last built SELF profile (held by the parent so it survives tab
   * switches). */
  profile: PlayerProfile | null;
  onProfileBuilt: (p: PlayerProfile) => void;
  /** Drill-down: load a database game at the given ply (game view). */
  onLoadGameAt: (gameId: number, ply: number) => void;
  /** Round-2 navigation contract (lib/shell.ts ViewParams): claim id to
   * pre-select in the evidence aside — "motif:<Kind>:missed" |
   * "motif:<Kind>:allowed" | "structure:<flag>". */
  claim?: string | null;
  /** Opponent subject (Prep's "Open his profile" navigates with it set). */
  opponent?: string | null;
  /** Shell navigation ("Train this weakness" seeds the tactics queue). */
  onNavigate: (view: ViewId, params?: ViewParams) => void;
}

type Subject = "self" | "opponent";

function motifTotal(m: MotifRow): number {
  return m.opportunities + m.taken + m.missed + m.allowed;
}

/** Default claim when nothing is selected: the loudest motif finding. */
function defaultClaim(p: PlayerProfile): Claim | null {
  const top = rankedMotifs(p)[0];
  if (!top) return null;
  return { kind: "motif", motif: top.kind, cell: top.missed >= top.allowed ? "missed" : "allowed" };
}

export default function ProfileView({
  initialPlayer,
  profile,
  onProfileBuilt,
  onLoadGameAt,
  claim,
  opponent,
  onNavigate,
}: ProfileViewProps) {
  const [subject, setSubject] = useState<Subject>(opponent ? "opponent" : "self");
  const [oppProfile, setOppProfile] = useState<PlayerProfile | null>(null);
  const [selected, setSelected] = useState<Claim | null>(parseClaim(claim));
  const [player, setPlayer] = useState(profile?.player ?? "");
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [building, setBuilding] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // The claim param is one-shot: re-parse whenever navigation changes it.
  useEffect(() => {
    const c = parseClaim(claim);
    if (c) setSelected(c);
  }, [claim]);

  // The app knows who you are (2026-07-30): seed the self field from the
  // database's canonical self_player instead of asking every session.
  useEffect(() => {
    if (player.trim() !== "") return;
    selfPlayerGet()
      .then((name) => {
        if (name) setPlayer((cur) => (cur.trim() === "" ? name : cur));
      })
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Opponent navigation switches the subject.
  useEffect(() => {
    if (opponent) setSubject("opponent");
  }, [opponent]);

  // Build the opponent's profile on demand when their tab is first shown.
  useEffect(() => {
    if (subject !== "opponent" || !opponent || oppProfile?.player === opponent) return;
    let stale = false;
    setBuilding(true);
    setError(null);
    buildProfile(opponent)
      .then((p) => !stale && setOppProfile(p))
      .catch((e) => !stale && setError(String(e)))
      .finally(() => !stale && setBuilding(false));
    return () => {
      stale = true;
    };
  }, [subject, opponent, oppProfile]);

  // Player-name suggestions for the self build form.
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

  // Identity forms merged into the current self profile (run 8.5):
  // shown so a false merge is never silent, editable via aliases.
  const [nameForms, setNameForms] = useState<NameForm[]>([]);
  const [aliasInput, setAliasInput] = useState("");
  const refreshForms = useCallback((name: string) => {
    identityGroup(name)
      .then(setNameForms)
      .catch(() => setNameForms([]));
  }, []);

  const buildSelf = useCallback(async () => {
    setBuilding(true);
    setError(null);
    try {
      const p = await buildProfile(player.trim());
      onProfileBuilt(p);
      // Home's findings read the cache — refresh it on every SELF build.
      cacheProfile(player.trim()).catch(() => {});
      // Building a self profile IS declaring who you are.
      selfPlayerSet(player.trim()).catch(() => {});
      setSelected((c) => c ?? defaultClaim(p));
      refreshForms(player.trim());
    } catch (e) {
      setError(String(e));
    } finally {
      setBuilding(false);
    }
  }, [player, onProfileBuilt, refreshForms]);

  // Deep-link auto-build (screenshots, shared links): run once.
  const autoBuilt = useRef(false);
  useEffect(() => {
    if (!initialPlayer || autoBuilt.current) return;
    autoBuilt.current = true;
    setPlayer(initialPlayer);
    (async () => {
      setBuilding(true);
      try {
        const prof = await buildProfile(initialPlayer);
        onProfileBuilt(prof);
        cacheProfile(initialPlayer).catch(() => {});
        setSelected((c) => c ?? defaultClaim(prof));
        refreshForms(initialPlayer);
      } catch (e) {
        setError(String(e));
      } finally {
        setBuilding(false);
      }
    })();
  }, [initialPlayer, onProfileBuilt, refreshForms]);

  const p = subject === "self" ? profile : oppProfile;
  const active = selected ?? (p ? defaultClaim(p) : null);

  /* ---- evidence resolution (claim → supporting games) ---- */
  const target = p && active ? claimTarget(p, active) : null;
  const [gameMeta, setGameMeta] = useState<Map<number, { title: string; date: string | null }>>(
    new Map(),
  );
  const metaRef = useRef(gameMeta);
  metaRef.current = gameMeta;
  const exampleKey = target ? target.examples.map((e) => e.game).join(",") : "";
  useEffect(() => {
    if (!target) return;
    const missing = target.examples.filter((e) => !metaRef.current.has(e.game));
    if (missing.length === 0) return;
    let stale = false;
    Promise.all(
      missing.map((e) =>
        getGame(e.game)
          .then((d) => ({
            id: e.game,
            title: `${d.white} — ${d.black} · ${d.event}`,
            date: d.date,
          }))
          .catch(() => ({ id: e.game, title: `game #${e.game}`, date: null })),
      ),
    ).then((rows) => {
      if (stale) return;
      setGameMeta((m) => {
        const next = new Map(m);
        for (const r of rows) next.set(r.id, { title: r.title, date: r.date });
        return next;
      });
    });
    return () => {
      stale = true;
    };
    // exampleKey captures the identity of target.examples.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [exampleKey]);

  const evidenceGames: EvidenceGame[] = (target?.examples ?? []).map((e: ProfileExample) => ({
    id: e.game,
    ply: e.ply,
    title: gameMeta.get(e.game)?.title ?? `game #${e.game}`,
    date: gameMeta.get(e.game)?.date ?? null,
  }));

  const select = (c: Claim) => setSelected(c);

  /* ---- motif matrix ---- */
  const motifRows = p ? p.motifs.filter((m) => motifTotal(m) > 0) : [];
  const isSel = (c: Claim) => sameClaim(active, c);
  const motifCols: DataTableColumn<MotifRow>[] = [
    {
      key: "motif",
      header: "MOTIF",
      render: (m) => <span className="pf2-motif-name">{shortMotif(m.kind)}</span>,
    },
    {
      key: "missed",
      header: "MISSED",
      align: "right",
      render: (m) => (
        <button
          type="button"
          className={`pf2-num${isSel({ kind: "motif", motif: m.kind, cell: "missed" }) ? " sel" : ""}`}
          onClick={(e) => {
            e.stopPropagation();
            select({ kind: "motif", motif: m.kind, cell: "missed" });
          }}
        >
          {m.missed}
        </button>
      ),
    },
    {
      key: "allowed",
      header: "ALLOWED",
      align: "right",
      render: (m) => (
        <button
          type="button"
          className={`pf2-num${m.allowed > 0 ? " bad" : ""}${isSel({ kind: "motif", motif: m.kind, cell: "allowed" }) ? " sel" : ""}`}
          onClick={(e) => {
            e.stopPropagation();
            select({ kind: "motif", motif: m.kind, cell: "allowed" });
          }}
        >
          {m.allowed}
        </button>
      ),
    },
    {
      key: "peers",
      header: "VS PEERS",
      align: "right",
      render: () => (
        <span className="pf2-peers" title="Peer baselines ship with the corpus profiling pass — not computed yet.">
          —
        </span>
      ),
    },
  ];

  /* ---- header ---- */
  const headerActions = (
    <>
      {opponent && (
        <div className="seg" role="tablist" aria-label="Profile subject">
          <button className={subject === "self" ? "cur" : ""} onClick={() => setSubject("self")}>
            You
          </button>
          <button
            className={subject === "opponent" ? "cur" : ""}
            onClick={() => setSubject("opponent")}
          >
            {opponent}
          </button>
        </div>
      )}
      {p && subject === "self" && (
        <button className="btn" onClick={() => void buildSelf()} disabled={building}>
          {building ? "Rebuilding…" : "Rebuild"}
        </button>
      )}
    </>
  );

  /* ---- empty / build states ---- */
  const buildForm = (
    <div className="pf2-build">
      <p className="pf2-build-prose">
        Build a profile from the open database: static screens plus stored evals — the engine
        never runs from here. Every number it produces opens its supporting games.
      </p>
      <div className="pf2-build-row">
        <input
          type="text"
          value={player}
          onChange={(e) => setPlayer(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && player.trim() !== "" && void buildSelf()}
          placeholder="your name as it appears in your games…"
          spellCheck={false}
        />
        <button
          className="btn-primary"
          onClick={() => void buildSelf()}
          disabled={building || player.trim() === ""}
        >
          {building ? "Building…" : "Build profile"}
        </button>
      </div>
      {suggestions.length > 0 && player.trim().length >= 2 && !suggestions.includes(player) && (
        <div className="pf2-suggest" role="listbox" aria-label="Matching players">
          {suggestions.slice(0, 8).map((name) => (
            <button key={name} type="button" className="pf2-suggest-row" onClick={() => setPlayer(name)}>
              {name}
            </button>
          ))}
        </div>
      )}
      <div className="hint">
        Names match as you type from the open database — pick yours, then Build.
      </div>
      {error && (
        <div className="error">
          {error}
          {error.includes("no database open") && (
            <div className="hint">Open a database from the Database rail item first.</div>
          )}
        </div>
      )}
    </div>
  );

  return (
    <div className="pf2">
      <ScreenHeader
        title="Profile"
        subtitle={
          p
            ? `${p.player} · ${p.games} games · eval coverage ${p.eval_coverage_pct.toFixed(1)}% — every number opens its supporting games`
            : "Engine-derived findings · every number opens its supporting games"
        }
        actions={headerActions}
      />
      <div className="pf2-body">
        <div className="pf2-content">
          {!p ? (
            subject === "opponent" ? (
              <div className="pf2-build">
                {building ? (
                  <p className="pf2-build-prose">Building {opponent}&rsquo;s profile…</p>
                ) : (
                  <p className="pf2-build-prose">
                    {error ?? `No profile for ${opponent} yet.`}
                  </p>
                )}
              </div>
            ) : (
              buildForm
            )
          ) : (
            <>
              <p className="pf2-lede">{profileLede(p)}</p>
              {subject === "self" && nameForms.length > 0 && (
                <div className="pf2-identity">
                  <span className="pf2-identity-label">INCLUDES</span>
                  {nameForms.map((f) => (
                    <span key={f.name} className="pf2-identity-form" title={f.games === 0 ? "declared alias — no imported games yet" : undefined}>
                      {f.name} <span className="pf2-identity-games">{f.games}g</span>
                      {nameForms.length > 1 && (
                        <button
                          className="pf2-identity-x"
                          title="Not the same person — split this name off (rebuild after)"
                          onClick={() => {
                            void aliasRemove(f.name).then(() => refreshForms(player.trim() || f.name));
                          }}
                        >
                          ×
                        </button>
                      )}
                    </span>
                  ))}
                  <span className="pf2-identity-add">
                    <input
                      type="text"
                      value={aliasInput}
                      onChange={(e) => setAliasInput(e.target.value)}
                      placeholder="also known as… (e.g. an online handle)"
                      spellCheck={false}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" && aliasInput.trim() !== "") {
                          void aliasDeclare(player.trim(), aliasInput.trim()).then((forms) => {
                            setNameForms(forms);
                            setAliasInput("");
                            void buildSelf(); // rebuild across the widened identity
                          });
                        }
                      }}
                    />
                  </span>
                </div>
              )}

              <div className="pf2-grid">
                <section className="pf2-panel">
                  <div className="pf2-panel-head">
                    <span className="pf2-panel-title">MOTIF MATRIX</span>
                    <span className="pf2-panel-note">
                      missed = you didn&rsquo;t play it · allowed = opponent got it
                    </span>
                  </div>
                  <DataTable
                    columns={motifCols}
                    rows={motifRows}
                    gridTemplate="1fr 66px 66px 74px"
                    rowKey={(m) => m.kind}
                    onRowClick={(m) =>
                      select({
                        kind: "motif",
                        motif: m.kind,
                        cell: m.missed >= m.allowed ? "missed" : "allowed",
                      })
                    }
                    rowClassName={(m) =>
                      active?.kind === "motif" && active.motif === m.kind ? "pf2-row-sel" : undefined
                    }
                    empty="No motif data — no medium+ alerts in the scanned games."
                  />
                </section>

                <section className="pf2-panel">
                  <div className="pf2-panel-head">
                    <span className="pf2-panel-title">STRUCTURE REPORT</span>
                  </div>
                  {p.structures.length === 0 ? (
                    <div className="pf2-empty">No recurring structure flags found.</div>
                  ) : (
                    <div className="pf2-structs">
                      {p.structures.map((s) => (
                        <button
                          key={s.flag}
                          type="button"
                          className={`pf2-struct${isSel({ kind: "structure", flag: s.flag }) ? " sel" : ""}`}
                          onClick={() => select({ kind: "structure", flag: s.flag })}
                        >
                          <span className="pf2-struct-row">
                            <span className="pf2-struct-name">{s.flag.replace(/-/g, " ")}</span>
                            <span className="pf2-struct-score">{s.score_pct.toFixed(0)}%</span>
                            <span className="pf2-struct-games">{s.games}g</span>
                          </span>
                          <BaselineBar
                            fraction={s.score_pct / 100}
                            tone={s.score_pct < 50 ? "bad" : "good"}
                            baseline={0.5}
                          />
                        </button>
                      ))}
                      <div className="pf2-struct-foot">
                        Bar = score in that structure; the tick is the 50% even-score baseline.
                      </div>
                    </div>
                  )}
                </section>
              </div>

              <div className="pf2-grid">
                <section className="pf2-panel">
                  <div className="pf2-panel-head">
                    <span className="pf2-panel-title">PHASE ACCURACY</span>
                  </div>
                  <div className="pf2-tiles3">
                    {(
                      [
                        ["OPENING", "opening", p.acpl_opening],
                        ["MIDDLEGAME", "middlegame", p.acpl_middlegame],
                        ["ENDGAME", "endgame", p.acpl_endgame],
                      ] as const
                    ).map(([label, phase, a]) => (
                      <StatTile
                        key={phase}
                        caption={label}
                        value={a.moves > 0 ? a.acpl.toFixed(0) : "—"}
                        unit="ACPL"
                        note={phaseNote(a)}
                        selected={isSel({ kind: "phase", phase })}
                        onClick={() => select({ kind: "phase", phase })}
                      />
                    ))}
                  </div>
                </section>

                <section className="pf2-panel">
                  <div className="pf2-panel-head">
                    <span className="pf2-panel-title">CONVERSION &amp; DEFENCE</span>
                  </div>
                  <div className="pf2-tiles2">
                    <StatTile
                      caption="CONVERSION FROM +2"
                      value={ratePct(p.conversion.converted_wins, p.conversion.winning_reached)}
                      note={`${p.conversion.winning_reached} winning positions · ${
                        p.conversion.winning_reached - p.conversion.converted_wins
                      } not converted`}
                      selected={isSel({ kind: "rate", rate: "conversion" })}
                      onClick={() => select({ kind: "rate", rate: "conversion" })}
                    />
                    <StatTile
                      caption="DEFENCE FROM −1"
                      value={ratePct(p.conversion.held, p.conversion.losing_reached)}
                      note={`${p.conversion.losing_reached} worse positions · ${p.conversion.held} held`}
                      selected={isSel({ kind: "rate", rate: "defence" })}
                      onClick={() => select({ kind: "rate", rate: "defence" })}
                    />
                  </div>
                  {p.eval_coverage_pct === 0 && (
                    <div className="pf2-struct-foot">
                      No engine evals stored — phase and conversion numbers need a Re-analyze +
                      Run jobs pass first.
                    </div>
                  )}
                </section>
              </div>
            </>
          )}
        </div>

        <EvidencePane
          countLabel={target?.countLabel ?? "—"}
          intro={
            target?.intro ??
            "Click any number on this screen — a motif cell, a structure bar, a phase or rate tile — and its supporting games appear here."
          }
          games={evidenceGames}
          onOpenGame={(g) => onLoadGameAt(g.id, g.ply)}
          empty={target?.emptyNote ?? "No supporting examples recorded for this claim."}
          footerNote={
            <>
              Every number on this screen opens its supporting games here; opening one jumps
              straight to the ply that produced the claim.
              {target && target.examples.length > 0 && target.total > target.examples.length
                ? ` Showing ${target.examples.length} of ${target.total} — examples are capped at three per claim.`
                : ""}
            </>
          }
          actions={
            <>
              <button
                className="btn-primary"
                disabled={!trainableMotif(active)}
                title={
                  trainableMotif(active)
                    ? "Seed the tactics queue with this motif"
                    : "Only motif claims can seed the tactics queue"
                }
                onClick={() => {
                  if (active) onNavigate("tactics", { claim: claimId(active) });
                }}
              >
                Train this weakness
              </button>
              <button
                className="btn-secondary"
                disabled={evidenceGames.length === 0}
                onClick={() => {
                  const g = evidenceGames[0];
                  if (g) onLoadGameAt(g.id, g.ply);
                }}
              >
                Open game
              </button>
            </>
          }
        />
      </div>
    </div>
  );
}
