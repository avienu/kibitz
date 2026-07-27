/**
 * Live-analysis controller (run-8 ruling): an explicit toggle in the game
 * view's engine area. While ON, the engine runs `go infinite` on the
 * current position and restarts whenever the position changes; toggling
 * OFF, changing games, or leaving the game view hard-stops it.
 *
 * The decision logic is pure so it can be tested without Tauri: events go
 * in, commands (`stop` / `start fen`) come out. The engine-off principle
 * governs DEFAULTS, not capabilities — the toggle always starts OFF and
 * is never persisted across sessions.
 */

export interface LiveState {
  on: boolean;
  /** FEN the engine is currently searching (null = not searching). */
  searching: string | null;
}

export const liveInitial: LiveState = { on: false, searching: null };

export type LiveCommand = { kind: "stop" } | { kind: "start"; fen: string };

export type LiveEvent =
  | { type: "toggle"; fen: string }
  | { type: "fenChanged"; fen: string }
  | { type: "leave" }; // game closed / view unmounted

/** Apply an event; returns the next state and the IPC commands to issue. */
export function liveReduce(
  state: LiveState,
  event: LiveEvent,
): { next: LiveState; commands: LiveCommand[] } {
  switch (event.type) {
    case "toggle": {
      if (state.on) {
        return {
          next: { on: false, searching: null },
          commands: [{ kind: "stop" }],
        };
      }
      return {
        next: { on: true, searching: event.fen },
        commands: [{ kind: "start", fen: event.fen }],
      };
    }
    case "fenChanged": {
      if (!state.on || state.searching === event.fen) {
        return { next: state, commands: [] };
      }
      // Restart on the new position: stop, then start. The backend stop
      // interrupts promptly; the start re-uses the running process.
      return {
        next: { on: true, searching: event.fen },
        commands: [{ kind: "stop" }, { kind: "start", fen: event.fen }],
      };
    }
    case "leave": {
      if (!state.on && state.searching === null) {
        return { next: state, commands: [] };
      }
      return {
        next: { on: false, searching: null },
        commands: [{ kind: "stop" }],
      };
    }
  }
}
