const PREFIXES = ["dr.", "dr", "özv.", "özv", "id.", "ifj.", "prof.", "vitéz"];

/** "dr. Kovács János" → ["dr.", "Kovács", "János"] (Hungarian name order, FR-ELH-08). */
export function split(full: string): [string, string, string] {
  const words = full.trim().split(/\s+/).filter(Boolean);
  const prefix: string[] = [];
  while (words.length && PREFIXES.includes(words[0].toLowerCase())) prefix.push(words.shift()!);
  return [prefix.join(" "), words[0] ?? "", words.slice(1).join(" ")];
}
