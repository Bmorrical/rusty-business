// A reply's Markdown, and its math, as HTML. Raw HTML in a reply is shown as
// text, never run: markdown-it is set not to pass it through, and KaTeX is
// set not to trust anything it is given.

import katex from "katex";
import MarkdownIt from "markdown-it";

const md = new MarkdownIt({ html: false, linkify: true, breaks: false, typographer: false });

// Math is $$...$$ or \[...\] on its own, and \(...\) in a line. A single $
// is left alone, since replies talk about money far more than math. Math is
// read by Markdown's own inline rules, so code in every form, fenced with
// backticks or tildes, indented, or inline, is left as written.
const OPENERS = [
  ["$$", "$$", true],
  ["\\[", "\\]", true],
  ["\\(", "\\)", false],
];

function math(tex, display) {
  try {
    return katex.renderToString(tex.trim(), { displayMode: display, throwOnError: false, trust: false, output: "html" });
  } catch {
    return md.utils.escapeHtml(tex);
  }
}

// Runs before the backslash-escape rule, which would read \( as (.
md.inline.ruler.before("escape", "math", (state, silent) => {
  for (const [open, close, display] of OPENERS) {
    if (!state.src.startsWith(open, state.pos)) continue;
    const start = state.pos + open.length;
    const end = state.src.indexOf(close, start);
    if (end < 0 || !state.src.slice(start, end).trim()) return false;
    if (!silent) {
      const token = state.push("math", "", 0);
      token.content = state.src.slice(start, end);
      token.meta = { display };
    }
    state.pos = end + close.length;
    return true;
  }
  return false;
});

// Display math stands as a block of its own, out of the paragraph it was in.
md.renderer.rules.math = (tokens, index, options, env) => {
  const { content, meta } = tokens[index];
  const html = math(content, meta.display);
  if (!meta.display) return html;
  const block = `<div class="math">${html}</div>`;
  return env.inParagraph ? `</p>\n${block}\n<p>` : block;
};

md.renderer.rules.paragraph_open = (tokens, index, options, env, self) => {
  env.inParagraph = !tokens[index].hidden;
  return self.renderToken(tokens, index, options);
};

/** The reply as HTML. */
export function renderMarkdown(text) {
  return md.render(String(text ?? ""), {}).replace(/<p>\s*<\/p>\n?/g, "");
}
