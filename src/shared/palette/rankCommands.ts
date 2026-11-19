export type Command = { id: string; title: string; keywords?: string[] };

/** 3 = prefix, 2 = contains, 1 = subsequence, 0 = no match. */
function score(text: string, q: string): number {
  const t = text.toLowerCase();
  if (t.startsWith(q)) return 3;
  if (t.includes(q)) return 2;
  let i = 0;
  for (const ch of t) if (ch === q[i] && ++i === q.length) return 1;
  return 0;
}

/**
 * Filters and orders commands for the palette. A title match always beats a keyword match; ties
 * keep registration order. An empty query returns everything in order.
 */
export function rankCommands<T extends Command>(commands: T[], query: string): T[] {
  const q = query.trim().toLowerCase();
  if (q === '') return commands;
  return commands
    .map((cmd, index) => {
      const title = score(cmd.title, q);
      const keyword = Math.max(0, ...(cmd.keywords ?? []).map((k) => score(k, q)));
      // Title scores occupy 4-7, keyword scores 1-3, so any title match outranks any keyword match.
      const rank = title > 0 ? title + 3 : keyword;
      return { cmd, index, rank };
    })
    .filter((r) => r.rank > 0)
    .sort((a, b) => b.rank - a.rank || a.index - b.index)
    .map((r) => r.cmd);
}
