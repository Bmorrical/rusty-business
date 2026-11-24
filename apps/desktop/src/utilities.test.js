import assert from "node:assert/strict";
import { test } from "node:test";

import {
  addPictures,
  folderName,
  heicConfirmation,
  isCheckable,
  isHeic,
  passedLine,
  percent,
  scoreLine,
  smallDownloadLine,
  splitDrop,
  pictureLine,
  pictureSize,
  pictureSummary,
  QUALITY,
  QUALITY_HIGH,
  QUALITY_LOW,
} from "./utilities.js";

// Brad, 2026-11-22: "Utilities Page, Mac only for now, strip first with a
// toggle to keep, take suggested"

test("a picture's size reads in KB under a megabyte and MB above", () => {
  assert.equal(pictureSize(830_000), "830 KB");
  assert.equal(pictureSize(120), "1 KB");
  assert.equal(pictureSize(2_400_000), "2.4 MB");
  assert.equal(pictureSize(undefined), "1 KB");
});

test("the quality slider starts at 90", () => {
  assert.equal(QUALITY, 90);
  assert.ok(QUALITY_LOW < QUALITY && QUALITY <= QUALITY_HIGH);
});

test("pictures join the list once each, and the list keeps what became of them", () => {
  const first = addPictures([], [
    { path: "/a/IMG_1.HEIC", name: "IMG_1.HEIC", bytes: 1 },
    { path: "/a/IMG_1.HEIC", name: "IMG_1.HEIC", bytes: 1 },
  ]);
  assert.equal(first.length, 1);
  assert.equal(first[0].status, "waiting");
  first[0].status = "done";
  const second = addPictures(first, [
    { path: "/a/IMG_1.HEIC", name: "IMG_1.HEIC", bytes: 1 },
    { path: "/a/IMG_2.HEIC", name: "IMG_2.HEIC", bytes: 2 },
  ]);
  assert.deepEqual(second.map((item) => [item.name, item.status]), [
    ["IMG_1.HEIC", "done"],
    ["IMG_2.HEIC", "waiting"],
  ]);
});

test("each line says the size before and after, or why it failed", () => {
  const item = { bytes: 2_400_000, outBytes: 1_100_000, outName: "IMG_1.jpg", error: "" };
  assert.equal(pictureLine({ ...item, status: "waiting" }), "2.4 MB, waiting");
  assert.equal(pictureLine({ ...item, status: "working" }), "2.4 MB, converting…");
  assert.equal(pictureLine({ ...item, status: "done" }), "2.4 MB to 1.1 MB, saved as IMG_1.jpg, found in the folder");
  assert.equal(pictureLine({ ...item, status: "failed", error: "The picture could not be read." }), "The picture could not be read.");
});

test("the summary counts what was converted and what failed", () => {
  assert.equal(pictureSummary([]), "");
  assert.equal(pictureSummary([{ status: "done" }, { status: "waiting" }]), "1 of 2 converted.");
  assert.equal(pictureSummary([{ status: "done" }, { status: "failed" }, { status: "done" }]), "2 of 3 converted, 1 failed.");
  assert.equal(passedLine(0), "");
  assert.equal(passedLine(1), "1 item was not a HEIC picture and was left out.");
  assert.equal(passedLine(3), "3 items were not HEIC pictures and were left out.");
});

// Brad, 2026-11-24: "heic worked great, but I need a confirmation I looked in
// folder and saw they converted."

const saved = (name, folder, keptPlace = false) => ({
  name: `${name}.HEIC`,
  status: "done",
  out: `${folder}/${name}.jpg`,
  outName: `${name}.jpg`,
  outFolder: folder,
  keptPlace,
  error: "",
});

test("no confirmation while a picture is waiting or converting, or before anything was tried", () => {
  assert.equal(heicConfirmation([]), null);
  assert.equal(heicConfirmation(undefined), null);
  assert.equal(heicConfirmation([saved("IMG_1", "/a/Trip"), { status: "working" }]), null);
  assert.equal(heicConfirmation([saved("IMG_1", "/a/Trip"), { status: "waiting" }]), null);
});

test("the confirmation says how many JPEGs were found in which folder, each folder once", () => {
  const trip = "/Users/me/Pictures/Trip";
  const shown = heicConfirmation([saved("IMG_1", trip), saved("IMG_2", "/Users/me/Desktop"), saved("IMG_3", trip)]);
  assert.equal(shown.headline, "Done. 3 JPEGs saved, each one found in its folder.");
  assert.deepEqual(
    shown.folders.map((one) => [one.line, one.folder, one.first.outName]),
    [
      ["2 JPEGs in Trip", trip, "IMG_1.jpg"],
      ["1 JPEG in Desktop", "/Users/me/Desktop", "IMG_2.jpg"],
    ],
  );
  assert.equal(shown.location, "Where each picture was taken was left out.");
  assert.deepEqual(shown.failed, []);
  assert.equal(heicConfirmation([saved("IMG_1", trip)]).headline, "Done. 1 JPEG saved and found in its folder.");
});

test("the confirmation names each picture that failed, and why", () => {
  const broken = { name: "IMG_9.HEIC", status: "failed", error: "IMG_9.HEIC could not be read." };
  const shown = heicConfirmation([saved("IMG_1", "/a/Trip"), broken]);
  assert.equal(shown.headline, "1 JPEG saved and found in its folder. 1 picture could not be converted.");
  assert.deepEqual(shown.failed, [{ name: "IMG_9.HEIC", error: "IMG_9.HEIC could not be read." }]);
  const none = heicConfirmation([broken, { ...broken, name: "IMG_8.HEIC", error: "" }]);
  assert.equal(none.headline, "2 pictures could not be converted.");
  assert.deepEqual(none.folders, []);
  assert.equal(none.location, "");
  assert.equal(none.failed[1].error, "Could not be converted.");
});

test("the confirmation says whether where each picture was taken was kept", () => {
  assert.equal(heicConfirmation([saved("IMG_1", "/a", true)]).location, "Where the picture was taken was kept.");
  assert.equal(heicConfirmation([saved("IMG_1", "/a")]).location, "Where the picture was taken was left out.");
  assert.equal(
    heicConfirmation([saved("IMG_1", "/a", true), saved("IMG_2", "/a", true), saved("IMG_3", "/a")]).location,
    "Where each picture was taken was kept in 2 JPEGs and left out of 1 JPEG.",
  );
});

test("a folder's name is the last part of its path", () => {
  assert.equal(folderName("/Users/me/Pictures/Trip"), "Trip");
  assert.equal(folderName("/Users/me/Pictures/Trip/"), "Trip");
  assert.equal(folderName("C:\\Users\\me\\Trip"), "Trip");
  assert.equal(folderName("/"), "/");
  assert.equal(folderName(undefined), "");
});

// Brad, 2026-11-22: "one click install on marqo should the user choose, and
// uninstall too"

test("the picture check reads JPEG, PNG, and WebP, and HEIC goes to HEIC to JPEG", () => {
  for (const name of ["a.jpg", "/x/B.JPEG", "c.png", "C:\\d\\e.WebP"]) assert.ok(isCheckable(name), name);
  for (const name of ["a.heic", "b.HEIF", "c.gif", "notes", ".png", ""]) assert.ok(!isCheckable(name), name);
  assert.ok(isHeic("/x/IMG_1.HEIC") && !isHeic("a.jpg"));
  assert.deepEqual(splitDrop(["/a.png", "/b.heic", "/folder", "/c.jpg"]), {
    check: ["/a.png", "/c.jpg"],
    rest: ["/b.heic", "/folder"],
  });
  assert.deepEqual(splitDrop(undefined), { check: [], rest: [] });
});

test("a score reads as the model's two numbers, the same way high or low", () => {
  assert.equal(scoreLine({ nsfw: 0.0845, sfw: 0.9155 }), "NSFW 8.5%, SFW 91.5%");
  assert.equal(scoreLine({ nsfw: 0.97, sfw: 0.03 }), "NSFW 97.0%, SFW 3.0%");
  assert.equal(percent(2), "100.0%");
  assert.equal(percent(undefined), "0.0%");
  for (const score of [{ nsfw: 0.01, sfw: 0.99 }, { nsfw: 0.99, sfw: 0.01 }]) {
    assert.doesNotMatch(scoreLine(score), /safe|unsafe|explicit|blocked|flagged|warning/i);
  }
});

test("the picture check's download reads in MB", () => {
  assert.equal(smallDownloadLine({ done: 3_100_000, total: 22_404_720 }), "3.1 of 22.4 MB");
  assert.equal(smallDownloadLine({ done: 99, total: 0 }), "0.0 of 0.0 MB");
  assert.equal(smallDownloadLine(null), "0.0 of 0.0 MB");
});
