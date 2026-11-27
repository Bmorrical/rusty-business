// The runners, as the Workflow view shows them: a slice handed to a model,
// with its clock.
//
// Brad, 2026-11-27, on what is left to build: "The orchestrator: handing
// slices to runners, with their time limits." On the API: "direct API
// connections are refused as rule for now".
//
// Rust picks nothing and trusts nothing from here: runners.rs refuses a model
// outside this computer, checks every rule before and after, and stops a
// runner at its limit. This file only words it.

import { isOutside } from "./chat.js";
import { nextLane } from "./board.js";

/** The rule that keeps a runner to its time, as rules.rs names it. */
export const ON_TIME = "on-time";

/**
 * The models a slice can go to: on this computer, on the LAN, or on the rig.
 * A model outside this computer is never one, nor one that reads pictures.
 */
export function runnerModels(models) {
  return (models ?? []).filter((entry) => entry && !entry.pictures && !isOutside(entry.id));
}

/** True when the slice's next lane is written, so a model can take it. */
export function canHandOff(slice) {
  return nextLane(slice)?.id === "written";
}

/** "4:05", or "1:02:05" past an hour. */
export function clock(ms) {
  const seconds = Math.max(0, Math.floor(Number(ms ?? 0) / 1000));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = String(seconds % 60).padStart(2, "0");
  if (hours) return `${hours}:${String(minutes).padStart(2, "0")}:${rest}`;
  return `${minutes}:${rest}`;
}

/** The time limit, in milliseconds, when the time rule is on; else null. */
export function limitOf(view) {
  const on = (view?.rules ?? []).some((rule) => rule.id === ON_TIME && rule.on);
  return on ? Number(view.limitMinutes) * 60_000 : null;
}

/** "Writing, 4:05 of 30:00", or "Writing, 4:05" with the time rule off. */
export function clockLine(ms, limit) {
  if (limit === null || limit === undefined) return `Writing, ${clock(ms)}`;
  return `Writing, ${clock(ms)} of ${clock(limit)}`;
}

/** What a run came to, in a line, with how long it took. */
export function ranLine(ran) {
  const words = String(ran?.words ?? "").trim();
  if (!ran?.tookMs) return words;
  return `${words} It took ${clock(ran.tookMs)}.`;
}
