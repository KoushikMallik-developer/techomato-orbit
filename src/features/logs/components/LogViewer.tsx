import { useLayoutEffect, useRef, useState } from 'react';
import type { LogLine } from '../logFilter';

const ROW = 20; // px, fixed so the visible window can be computed without measuring
const OVERSCAN = 20;

function time(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour12: false });
}

/** Virtualised: only the rows in view (plus a margin) are in the DOM, however long the log is. */
export function LogViewer({ lines, follow }: { lines: LogLine[]; follow: boolean }) {
  const box = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(480);

  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    setHeight(el.clientHeight);
    const ro = new ResizeObserver(() => setHeight(el.clientHeight));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  useLayoutEffect(() => {
    const el = box.current;
    if (follow && el) el.scrollTop = el.scrollHeight;
  }, [lines, follow]);

  const first = Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN);
  const last = Math.min(lines.length, Math.ceil((scrollTop + height) / ROW) + OVERSCAN);

  return (
    <div
      ref={box}
      role="log"
      aria-label="Log output"
      aria-live="off"
      tabIndex={0}
      onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
      className="h-full min-h-64 overflow-auto rounded-card border border-border bg-paper font-mono text-xs"
    >
      <div style={{ height: lines.length * ROW, position: 'relative' }}>
        {lines.slice(first, last).map((l, i) => (
          <div
            key={first + i}
            style={{ position: 'absolute', top: (first + i) * ROW, height: ROW }}
            className={`flex w-full items-center gap-3 whitespace-pre px-3 ${
              l.stream === 'stderr' ? 'text-danger' : 'text-fg'
            }`}
          >
            <span className="shrink-0 text-fg-subtle">{time(l.tsMs)}</span>
            {l.stream === 'stderr' && <span className="sr-only">error: </span>}
            <span className="truncate">{l.text}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
