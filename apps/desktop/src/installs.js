// Other Rusty Business™ installs on your network, as the page offers them.
//
// Brad, 2026-11-25: "one [Rusty Business™] can talk to another [Rusty
// Business™] on the same LAN, it has it's own connection handshake", and
// "anything bubbled back ... goes to rb to rb back to me". Then, 2026-11-27:
// "The handshake".
//
// Rust pairs, signs every request, and shows an answer only once it proves
// it came from the install paired (installs.rs and handshake.rs). This file
// only words it, and offers the models while they can be asked.

/** What an install's model's chat id starts with. */
export const INSTALL_PREFIX = "install:";

/** True for a chat id of a model on another install. */
export function isInstall(id) {
  return String(id ?? "").startsWith(INSTALL_PREFIX);
}

/**
 * The models the installs offer, ready to pick, only while the circuit
 * breaker and the LAN gate are on. Each answers at once, as a rig model does.
 */
export function installModels(found, ready) {
  if (!ready) return [];
  return (found ?? []).flatMap((install) =>
    (install?.models ?? []).map((model) => ({
      id: `${INSTALL_PREFIX}${install.id}/${model}`,
      name: `${model} on ${install.name}`,
      maker: install.name,
      install: true,
      place: install.name,
    })),
  );
}

/** Why the installs' models are or are not offered, in a line. */
export function installsLine(breakerOn, lanOpen) {
  if (!breakerOn) return "The circuit breaker is off, so no other install is asked.";
  if (!lanOpen) return "The LAN gate is off, so no other install is asked.";
  return "The models on installs you pair are offered with your own.";
}

/** An install's trouble, when one did not answer, for the list. */
export function troubleOf(found, id) {
  return (found ?? []).find((install) => install?.id === id)?.error ?? "";
}

/** True when the pairing form can be sent. */
export function canPair(draft) {
  return Boolean(draft?.address?.trim()) && /^\d{6}$/.test(String(draft?.code ?? "").replace(/\s/g, "")) && Boolean(draft?.name?.trim());
}
