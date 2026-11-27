// The orchestrator's rules, as the Rules tab shows them.
//
// Brad, 2026-11-27: "it's a rule of the rusty business™ code, and it's enforced
// by toggle, and it's used in workflows".
//
// The rules themselves are coded in rules.rs, with their tests. The page shows
// each with its toggle, and the runner's time limit; Rust checks and keeps them.

/** The shortest and the longest time limit, in minutes, as rules.rs keeps them. */
export const LIMIT_LEAST = 5;
export const LIMIT_MOST = 480;

/** What a rule's switch says: the rule with no toggle is always on. */
export function switchWord(rule) {
  if (rule.locked) return "Always on";
  return rule.on ? "On" : "Off";
}

/** The time limit typed in the box, as whole minutes, or null when it is not one. */
export function limitOf(value) {
  const text = String(value ?? "").trim();
  if (!/^\d+$/.test(text)) return null;
  const minutes = Number(text);
  if (minutes < LIMIT_LEAST || minutes > LIMIT_MOST) return null;
  return minutes;
}
