/// A unit's multiplier to the base one: time to milliseconds, volume and rate to bytes.
const UNITS: Record<string, number> = {
  ms: 1,
  s: 1000,
  min: 60_000,
  мс: 1,
  с: 1000,
  мин: 60_000,
  B: 1,
  KB: 1024,
  MB: 1024 ** 2,
  GB: 1024 ** 3,
  Б: 1,
  КБ: 1024,
  МБ: 1024 ** 2,
  ГБ: 1024 ** 3,
  "%": 1,
};

const MEASURE = /^(-?\d+(?:[.,]\d+)?)\s*([^\d\s/]*)(?:\/[sс])?$/;

/**
 * A number from a report cell — to sort quantities rather than strings (D-097).
 *
 * The utility sends ready text ("1.1 s", "235 ms"), and as a string "1.1 s" is less than
 * "235 ms". Empty is not a quantity: a column with such a cell sorts as text.
 */
export function measure(text: string): number | null {
  const match = MEASURE.exec(text.trim());
  if (!match) return null;
  const [, digits, unit] = match;
  const scale = unit === "" ? 1 : UNITS[unit];
  if (scale === undefined) return null;
  return Number(digits.replace(",", ".")) * scale;
}
