import { useEffect, useMemo } from "react";
import guideSource from "../../docs/USER_GUIDE.md?raw";
import { parseMarkdown, type Block, type InlineSpan } from "./lib/markdown";

interface HelpProps {
  onClose: () => void;
}

function Spans({ spans }: { spans: InlineSpan[] }) {
  return (
    <>
      {spans.map((s, i) =>
        s.code ? (
          <code key={i}>{s.text}</code>
        ) : s.bold ? (
          <strong key={i}>{s.text}</strong>
        ) : (
          <span key={i}>{s.text}</span>
        ),
      )}
    </>
  );
}

function BlockView({ block }: { block: Block }) {
  switch (block.kind) {
    case "heading": {
      const Tag = (["h1", "h2", "h3", "h4"] as const)[block.level - 1];
      return (
        <Tag>
          <Spans spans={block.spans} />
        </Tag>
      );
    }
    case "para":
      return (
        <p>
          <Spans spans={block.spans} />
        </p>
      );
    case "code":
      return (
        <pre>
          <code>{block.text}</code>
        </pre>
      );
    case "list": {
      const items = block.items.map((item, i) => (
        <li key={i}>
          <Spans spans={item} />
        </li>
      ));
      return block.ordered ? <ol>{items}</ol> : <ul>{items}</ul>;
    }
    case "rule":
      return <hr />;
  }
}

/**
 * In-app user guide (run-5 item 6): renders docs/USER_GUIDE.md, bundled
 * into the app at build time via Vite's `?raw` import — no runtime fetch.
 */
export default function Help({ onClose }: HelpProps) {
  const blocks = useMemo(() => parseMarkdown(guideSource), []);

  // Escape closes the guide.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal help-modal" onClick={(e) => e.stopPropagation()}>
        <div className="help-header">
          <h3>User guide</h3>
          <button onClick={onClose}>Close</button>
        </div>
        <div className="help-body">
          {blocks.map((b, i) => (
            <BlockView key={i} block={b} />
          ))}
        </div>
      </div>
    </div>
  );
}
