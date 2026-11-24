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
    added.push({ ...picture, status: "waiting", out: "", outName: "", outBytes: 0, outFolder: "", keptPlace: false, error: "" });
  }
  return [...list, ...added];
}

/** What one picture on the list is doing, or what it became. */
export function pictureLine(item) {
  if (item.status === "working") return `${pictureSize(item.bytes)}, converting…`;
  if (item.status === "done") return `${pictureSize(item.bytes)} to ${pictureSize(item.outBytes)}, saved as ${item.outName}, found in the folder`;
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

/** "Trip", for "/Users/me/Pictures/Trip": the last part of a folder's path. */
export function folderName(folder) {
  const parts = String(folder ?? "").split(/[\\/]/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : String(folder ?? "");
}

/** "1 JPEG", "3 JPEGs". */
function jpegs(count) {
  return count === 1 ? "1 JPEG" : `${count} JPEGs`;
}

/**
 * What the run saved, and where, once nothing on the list is waiting or
 * converting. Brad, 2026-11-24: "heic worked great, but I need a confirmation
 * I looked in folder and saw they converted." A picture is done only after
 * its JPEG was read back from its folder, so each one counted here was found
 * there. Null while the run goes on, and while nothing has been tried.
 */
export function heicConfirmation(list) {
  const items = list ?? [];
  if (items.some((item) => item.status === "waiting" || item.status === "working")) return null;
  const done = items.filter((item) => item.status === "done");
  const failed = items.filter((item) => item.status === "failed");
  if (!done.length && !failed.length) return null;

  const folders = [];
  for (const item of done) {
    const known = folders.find((one) => one.folder === item.outFolder);
    if (known) known.count += 1;
    else folders.push({ folder: item.outFolder, name: folderName(item.outFolder), count: 1, first: item });
  }
  for (const one of folders) one.line = `${jpegs(one.count)} in ${one.name}`;

  let saved = "No JPEGs were saved.";
  if (done.length === 1) saved = "1 JPEG saved and found in its folder.";
  else if (done.length) saved = `${done.length} JPEGs saved, each one found in its folder.`;
  const lost = failed.length === 1 ? "1 picture could not be converted." : `${failed.length} pictures could not be converted.`;
  let headline = `Done. ${saved}`;
  if (failed.length) headline = done.length ? `${saved} ${lost}` : lost;

  const kept = done.filter((item) => item.keptPlace).length;
  const left = done.length - kept;
  const which = done.length === 1 ? "the picture" : "each picture";
  let location = "";
  if (done.length && !kept) location = `Where ${which} was taken was left out.`;
  else if (done.length && !left) location = `Where ${which} was taken was kept.`;
  else if (done.length) location = `Where each picture was taken was kept in ${jpegs(kept)} and left out of ${jpegs(left)}.`;

  return {
    headline,
    folders,
    location,
    failed: failed.map((item) => ({ name: item.name, error: item.error || "Could not be converted." })),
  };
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
