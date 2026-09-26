/** D-148: newlines separate entries; commas and spaces inside values stay literal. */
export function ruleValues(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}
