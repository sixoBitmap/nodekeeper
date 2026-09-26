/**
 * What to show for a line the backend refused to even classify (a parse
 * error, a pasted `bitcoin-cli` prefix). Only its leading word, and only if
 * that is made of plain letters, digits, `-` and `_`: the rest of the line may
 * be a passphrase or a key that nothing has hidden yet. (Deliberately not
 * "split on whitespace": the backend's tokenizer and a browser disagree on
 * what whitespace is -- U+0085, for one -- and whatever the browser did not
 * split on would be shown.)
 */
export function refusedLineLabel(commandLine: string): string {
  const line = commandLine.trim();
  const word = /^[A-Za-z0-9_-]*/.exec(line)?.[0] ?? "";
  return line.length > word.length ? `${word} …` : word;
}
