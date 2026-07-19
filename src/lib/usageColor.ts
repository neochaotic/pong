/**
 * A continuous gradient for a usage bar, not three fixed buckets: blue and
 * unremarkable while there's headroom, warming through amber as it climbs,
 * red past 85%, and a deliberately darker red as it tightens further. 99%+
 * gets its own treatment (pulse + ring, applied by the caller) rather than
 * just being "the reddest red" — the one state that should interrupt you,
 * not just inform you.
 */

type Rgb = { r: number; g: number; b: number };

const BLUE: Rgb = { r: 0x5e, g: 0x6a, b: 0xd2 }; // the brand accent ("signal")
// Darker than the app's "warn" yellow on purpose — climbing through 60-85%
// should read as "warming up", not already alarming.
const AMBER: Rgb = { r: 0xb4, g: 0x7b, b: 0x10 };
const RED: Rgb = { r: 0xeb, g: 0x57, b: 0x57 }; // danger
const DEEP_RED: Rgb = { r: 0x9b, g: 0x1c, b: 0x1c };

function lerp(a: number, b: number, t: number): number {
  return Math.round(a + (b - a) * t);
}

function mix(from: Rgb, to: Rgb, t: number): string {
  const c = Math.min(1, Math.max(0, t));
  return `rgb(${lerp(from.r, to.r, c)}, ${lerp(from.g, to.g, c)}, ${lerp(from.b, to.b, c)})`;
}

/** An `rgb(...)` CSS color for a usage percentage, 0-100 (values above 100 clamp). */
export function barColor(percent: number): string {
  if (percent <= 60) return mix(BLUE, BLUE, 0);
  if (percent <= 85) return mix(BLUE, AMBER, (percent - 60) / 25);
  if (percent <= 90) return mix(AMBER, RED, (percent - 85) / 5);
  if (percent < 99) return mix(RED, DEEP_RED, (percent - 90) / 9);
  return mix(DEEP_RED, DEEP_RED, 1);
}

/** Essentially no headroom left. */
export function isCritical(percent: number): boolean {
  return percent >= 99;
}
