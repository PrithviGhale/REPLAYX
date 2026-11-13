/** Replay length slider: 30 seconds to 10 minutes in 15-second steps, then
 * 15 and 30 minutes (the longest lengths, also kept from earlier versions).
 * The engine accepts any length from 30 s to 30 min (`Settings::validate`).
 *
 * The slider moves over positions, not seconds, so 15 and 30 minutes get a
 * wide, easy-to-hit stretch at its end. Every position snaps to a stop; the
 * thumb, the label, the screen-reader value and the saved setting all come
 * from that stop. A saved length that isn't a stop (written by another
 * version) is added as a stop of its own, so it is shown exactly and never
 * changed until the user moves the slider. */
export const MIN_LENGTH = 30;
export const MAX_LENGTH = 1800;
const STEP = 15;
const SHORT_MAX = 600;
const LONG = [900, 1800];
/** Positions between two long stops (one per 15-second step below 10 min). */
const LONG_GAP = 6;

export type LengthScale = { stops: number[]; positions: number[]; max: number };

export function lengthScale(current: number): LengthScale {
  const stops = [...LONG];
  for (let s = MIN_LENGTH; s <= SHORT_MAX; s += STEP) stops.push(s);
  if (current >= MIN_LENGTH && current <= MAX_LENGTH && !stops.includes(current)) stops.push(current);
  stops.sort((a, b) => a - b);
  const positions: number[] = [];
  stops.forEach((s, i) => positions.push(i === 0 ? 0 : positions[i - 1] + (s > SHORT_MAX ? LONG_GAP : 1)));
  return { stops, positions, max: positions[positions.length - 1] };
}

/** Slider position of a length (a stop of this scale). */
export function positionOf(scale: LengthScale, seconds: number): number {
  const i = scale.stops.indexOf(seconds);
  if (i >= 0) return scale.positions[i];
  // Not a stop (out of range): the nearest end, for display only.
  return seconds < MIN_LENGTH ? 0 : scale.max;
}

/** The stop nearest a slider position. */
export function lengthAt(scale: LengthScale, position: number): number {
  let best = 0;
  scale.positions.forEach((p, i) => { if (Math.abs(p - position) < Math.abs(scale.positions[best] - position)) best = i; });
  return scale.stops[best];
}

/** Move `by` stops from a length (arrow keys ±1, Page keys ±4), staying in range. */
export function stepLength(scale: LengthScale, seconds: number, by: number): number {
  const i = scale.stops.indexOf(seconds);
  const from = i >= 0 ? i : by > 0 ? -1 : scale.stops.length;
  return scale.stops[Math.min(scale.stops.length - 1, Math.max(0, from + by))];
}

/** Labels under the slider, at their real positions (percent of the track). */
export function lengthMarks(scale: LengthScale): { seconds: number; percent: number }[] {
  return [MIN_LENGTH, 300, SHORT_MAX, ...LONG].map(seconds => ({ seconds, percent: (positionOf(scale, seconds) / scale.max) * 100 }));
}
