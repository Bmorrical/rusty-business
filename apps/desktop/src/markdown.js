// A reply's Markdown, and its math, as HTML. Raw HTML in a reply is shown as
// text, never run: markdown-it is set not to pass it through, and KaTeX is
// set not to trust anything it is given.

import katex from "katex";
import MarkdownIt from "markdown-it";

const md = new MarkdownIt({ html: false, linkify: true, breaks: false, typographer: false });

// Code fences and inline code are kept whole, so math inside them is left as
// written. Math is $$...$$ or \[...\] on its own, and \(...\) in a line. A
// single $ is left alone, since replies talk about money far more than math.
const PIECES = /(```[\s\S]*?(?:```|$)|`[^`\n]*`)|\$\$([\s\S]+?)\$\$|\\\[([\s\S]+?)\\\]|\\\(([\s\S]+?)\\\)/g;

function math(tex, display) {
  try {
    return katex.renderToString(tex.trim(), { displayMode: display, throwOnError: false, trust: false, output: "html" });
  } catch {
    return md.utils.escapeHtml(tex);
  }
}

/** The reply as HTML. */
export function renderMarkdown(text) {
  const found = [];
  const marked = String(text ?? "").replace(PIECES, (whole, code, dollars, bracket, paren) => {
    if (code !== undefined) return whole;
    const display = paren === undefined;
    found.push(math(dollars ?? bracket ?? paren, display));
    const mark = `MATHMARK${found.length - 1}END`;
    return display ? `\n\n${mark}\n\n` : mark;
  });
  return md.render(marked).replace(/(<p>)?MATHMARK(\d+)END(<\/p>)?/g, (whole, open, index, close) => {
    const html = found[Number(index)] ?? "";
    return open && close ? `<div class="math">${html}</div>` : `${open ?? ""}${html}${close ?? ""}`;
  });
}
