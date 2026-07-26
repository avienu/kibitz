import { describe, expect, it } from "vitest";
import {
  ANNOTATION_DISPLAY_MODES,
  commentView,
  getSavedAnnotationDisplay,
} from "./annotationDisplay";

describe("commentView", () => {
  it("full mode shows the comment text inline", () => {
    const v = commentView("full", "best by test");
    expect(v).toEqual({ visible: true, collapsed: false, text: "best by test" });
  });

  it("hover mode collapses to a ° marker with the text as tooltip", () => {
    const v = commentView("hover", "best by test");
    expect(v.visible).toBe(true);
    expect(v.collapsed).toBe(true);
    expect(v.text).toBe("°");
    expect(v.title).toBe("best by test");
  });

  it("hidden mode renders nothing", () => {
    const v = commentView("hidden", "best by test");
    expect(v.visible).toBe(false);
  });

  it("covers every declared mode", () => {
    for (const mode of ANNOTATION_DISPLAY_MODES) {
      expect(commentView(mode, "x")).toBeDefined();
    }
  });
});

describe("getSavedAnnotationDisplay", () => {
  it("defaults to full when no persistence layer exists", () => {
    // vitest runs in node: localStorage is absent, the guard must kick in.
    expect(getSavedAnnotationDisplay()).toBe("full");
  });
});
