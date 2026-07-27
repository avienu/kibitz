/**
 * Annotation-display toggle view model (run-4 verdict 4): the game view
 * shows comments in full, collapsed to a ° marker revealed on hover, or
 * hidden entirely. The choice persists in localStorage.
 *
 * No DOM assumptions beyond optional localStorage — unit-testable.
 */

export type AnnotationDisplay = "full" | "hover" | "hidden";

export const ANNOTATION_DISPLAY_MODES: readonly AnnotationDisplay[] = ["full", "hover", "hidden"];

const KEY = "kibitz.annotationDisplay";

export function getSavedAnnotationDisplay(): AnnotationDisplay {
  try {
    const v = localStorage.getItem(KEY);
    return v === "hover" || v === "hidden" ? v : "full";
  } catch {
    return "full";
  }
}

export function saveAnnotationDisplay(mode: AnnotationDisplay): void {
  try {
    localStorage.setItem(KEY, mode);
  } catch {
    // Non-browser environment: nothing to persist.
  }
}

/** How the move list should render one comment under `mode`. */
export interface CommentRender {
  /** Render anything at all? */
  visible: boolean;
  /** Render as the ° marker (full text in the tooltip) instead of inline text. */
  collapsed: boolean;
  /** Text to render inline. */
  text: string;
  /** Tooltip (collapsed mode only). */
  title?: string;
}

export function commentView(mode: AnnotationDisplay, text: string): CommentRender {
  switch (mode) {
    case "full":
      return { visible: true, collapsed: false, text };
    case "hover":
      return { visible: true, collapsed: true, text: "°", title: text };
    case "hidden":
      return { visible: false, collapsed: false, text: "" };
  }
}
