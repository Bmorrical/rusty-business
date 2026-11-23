// The Utilities page: small tools that run on this computer alone.
//
// Brad, 2026-11-22: "I want a new "utilities" page or button, and it's job is
// to list helpful utilities I used all the time. convert HEIC to JPEG is
// goal." Then: "Utilities Page, Mac only for now, strip first with a toggle to
// keep, take suggested".

/** The JPEG quality the slider starts at, and its ends. */
export const QUALITY = 90;
export const QUALITY_LOW = 50;
export const QUALITY_HIGH = 100;

/** "830 KB", "2.4 MB": a picture's size. */
export function pictureSize(bytes) {
  const value = Number(bytes ?? 0);
  if (value < 1e6) return `${Math.max(1, Math.round(value / 1e3))} KB`;
  return `${(value / 1e6).toFixed(1)} MB`;
}

/**
 * The list with the pictures found added at the end, each once. A picture
 * already on the list keeps its place and what became of it.
 */
export function addPictures(list, found) {
  const known = new Set(list.map((item) => item.path));
  const added = [];
  for (const picture of found ?? []) {
    if (known.has(picture.path)) continue;
    known.add(picture.path);
    added.push({ ...picture, status: "waiting", out: "", outName: "", outBytes: 0, error: "" });
  }
  return [...list, ...added];
}

/** What one picture on the list is doing, or what it became. */
export function pictureLine(item) {
  if (item.status === "working") return `${pictureSize(item.bytes)}, converting…`;
  if (item.status === "done") return `${pictureSize(item.bytes)} to ${pictureSize(item.outBytes)}, saved as ${item.outName}`;
  if (item.status === "failed") return item.error || "Could not be converted.";
  return `${pictureSize(item.bytes)}, waiting`;
}

/** "2 of 3 converted, 1 failed." Empty while nothing is on the list. */
export function pictureSummary(list) {
  if (!list.length) return "";
  const done = list.filter((item) => item.status === "done").length;
  const failed = list.filter((item) => item.status === "failed").length;
  const words = `${done} of ${list.length} converted`;
  return failed ? `${words}, ${failed} failed.` : `${words}.`;
}

/** Said when a drop or a pick held files that are not HEIC pictures. */
export function passedLine(passed) {
  if (!passed) return "";
  return passed === 1
    ? "1 item was not a HEIC picture and was left out."
    : `${passed} items were not HEIC pictures and were left out.`;
}

// The picture check. Brad, 2026-11-22: "one click install on marqo should the
// user choose, and uninstall too". The score is shown on the page and kept
// nowhere: not in a chat, not in the browser's storage, and not in a log.

/** The kinds of picture the check reads. HEIC goes to HEIC to JPEG first. */
export const CHECKABLE = ["jpg", "jpeg", "png", "webp"];

/** The file's kind, from its name, in lower case. */
function kindOf(path) {
  const name = String(path ?? "").split(/[\\/]/).pop();
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/** True for a JPEG, PNG, or WebP picture, by its name. */
export function isCheckable(path) {
  return CHECKABLE.includes(kindOf(path));
}

/** True for a HEIC or HEIF picture, by its name. */
export function isHeic(path) {
  return ["heic", "heif"].includes(kindOf(path));
}

/** A dropped list, split: the pictures to check, and the rest. */
export function splitDrop(paths) {
  const check = [];
  const rest = [];
  for (const one of paths ?? []) (isCheckable(one) ? check : rest).push(one);
  return { check, rest };
}

/** "8.5%": one of the model's numbers, as a share. */
export function percent(value) {
  const share = Math.min(1, Math.max(0, Number(value) || 0)) * 100;
  return `${share.toFixed(1)}%`;
}

/**
 * The score in the model's own words, never a verdict: "NSFW 8.5%, SFW
 * 91.5%". A high score reads the same way as a low one.
 */
export function scoreLine(score) {
  return `NSFW ${percent(score?.nsfw)}, SFW ${percent(score?.sfw)}`;
}

/** "3.1 of 22.4 MB": how far the picture check's download has come. */
export function smallDownloadLine(progress) {
  const total = Number(progress?.total ?? 0);
  const done = Math.min(Number(progress?.done ?? 0), total);
  return `${(done / 1e6).toFixed(1)} of ${(total / 1e6).toFixed(1)} MB`;
}
