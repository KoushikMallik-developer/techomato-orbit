export type Stream = 'stdout' | 'stderr';
export type LogLine = { tsMs: number; stream: Stream; text: string };

/** Case-insensitive literal substring match, limited to the chosen streams. */
export function filterLines(
  lines: LogLine[],
  query: string,
  streams: ReadonlySet<Stream>,
): LogLine[] {
  const q = query.toLowerCase();
  return lines.filter(
    (l) => streams.has(l.stream) && (q === '' || l.text.toLowerCase().includes(q)),
  );
}
