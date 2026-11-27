// The rig: many models at once on servers on your private network.
//
// Brad, 2026-11-24: "many models at once on the LAN, a pie per card, a slider
// per model, shares can span cards".
//
// The page edits a draft of the plan: each model has one slider per card,
// from 0 to 100 percent. Rust checks the plan when it is saved (rig.rs), and
// works out what it comes to. These helpers turn the draft into the plan and
// back, and draw each card's pie.

/** The most cards and models a plan holds, as rig.rs keeps them. */
export const CARDS_MOST = 4;
export const MODELS_MOST = 16;
/** How far one move of a slider goes. */
export const SHARE_STEP = 5;

/** The colors of a pie's slices, one per model in the plan's order. */
export const SLICE_COLORS = ["#ff5b1f", "#2f7d4f", "#3f6fd8", "#b8860b", "#8e5bd0", "#0f8f8f"];

/** The draft the page edits, from a kept plan. */
export function draftOf(plan) {
  const cards = (plan?.cards ?? []).map((card) => ({ name: card.name, memoryGb: card.memoryGb }));
  const models = (plan?.models ?? []).map((model) => ({
    id: model.id,
    name: model.name,
    address: model.address,
    model: model.model,
    keyFile: model.keyFile ?? "",
    needsGb: model.needsGb ?? "",
    percents: cards.map((_, place) => model.shares?.find((share) => share.card === place)?.percent ?? 0),
  }));
  return { cards, models };
}

/** The plan to save, from the draft: a share for each slider above 0. */
export function planOf(draft) {
  return {
    cards: draft.cards.map((card) => ({ name: String(card.name ?? ""), memoryGb: Number(card.memoryGb) })),
    models: draft.models.map((model) => {
      const needs = String(model.needsGb ?? "").trim();
      const keyFile = String(model.keyFile ?? "").trim();
      return {
        id: model.id ?? "",
        name: String(model.name ?? ""),
        address: String(model.address ?? ""),
        model: String(model.model ?? ""),
        keyFile: keyFile || null,
        needsGb: needs ? Number(needs) : null,
        shares: model.percents
          .map((percent, card) => ({ card, percent: Number(percent) }))
          .filter((share) => share.percent > 0),
      };
    }),
  };
}

/** A new card, at the end. */
export function withCard(draft) {
  if (draft.cards.length >= CARDS_MOST) return draft;
  return {
    cards: [...draft.cards, { name: `Card ${draft.cards.length + 1}`, memoryGb: 24 }],
    models: draft.models.map((model) => ({ ...model, percents: [...model.percents, 0] })),
  };
}

/** The draft without one card, and without each model's share of it. */
export function withoutCard(draft, place) {
  return {
    cards: draft.cards.filter((_, index) => index !== place),
    models: draft.models.map((model) => ({ ...model, percents: model.percents.filter((_, index) => index !== place) })),
  };
}

/** A new model, with no share yet. */
export function withModel(draft) {
  if (draft.models.length >= MODELS_MOST || !draft.cards.length) return draft;
  const model = { id: "", name: "", address: "", model: "", keyFile: "", needsGb: "", percents: draft.cards.map(() => 0) };
  return { cards: draft.cards, models: [...draft.models, model] };
}

/** The draft without one model. */
export function withoutModel(draft, place) {
  return { cards: draft.cards, models: draft.models.filter((_, index) => index !== place) };
}

/** What is left of a card for one model's slider: its own share and what is free. */
export function roomFor(draft, model, card) {
  const others = draft.models.reduce((sum, other, index) => (index === model ? sum : sum + Number(other.percents[card] ?? 0)), 0);
  return Math.max(0, 100 - others);
}

/** One card's pie: each model's slice, in the plan's order, and what is free. */
export function pieOf(draft, card) {
  const slices = [];
  let used = 0;
  draft.models.forEach((model, index) => {
    const percent = Number(model.percents[card] ?? 0);
    if (percent <= 0) return;
    slices.push({ name: model.name || `Model ${index + 1}`, percent, color: SLICE_COLORS[index % SLICE_COLORS.length] });
    used += percent;
  });
  return { slices, free: Math.max(0, 100 - used), over: used > 100 };
}

/** The pie as a CSS conic gradient: the slices, then what is free. */
export function pieStyle(pie) {
  let from = 0;
  const stops = pie.slices.map((slice) => {
    const to = Math.min(100, from + slice.percent);
    const stop = `${slice.color} ${from}% ${to}%`;
    from = to;
    return stop;
  });
  stops.push(`var(--hover) ${from}% 100%`);
  return `background: conic-gradient(${stops.join(", ")});`;
}

/** "1.25 cards, 108 GB": what a model's shares come to. */
export function shareLine(view) {
  if (!view) return "";
  const cards = view.cards === 1 ? "1 card" : `${view.cards} cards`;
  const line = `${cards}, ${view.memoryGb} GB`;
  if (view.room === false) return `${line}, less than it needs`;
  return line;
}

/** The models the chat offers from the rig, while the LAN gate is on. */
export function rigModels(view, lanOpen) {
  if (!lanOpen) return [];
  return (view?.models ?? []).map((model) => ({
    id: model.chatId,
    name: model.name,
    maker: model.listedAs,
    context: model.context,
    rig: true,
    place: model.place,
  }));
}
