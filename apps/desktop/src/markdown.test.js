import assert from "node:assert/strict";
import { test } from "node:test";

import { renderMarkdown } from "./markdown.js";

test("a reply's Markdown becomes HTML, and its HTML stays text", () => {
  const html = renderMarkdown("### Evidence\n\n1. **Studies**: more\n2. Models\n\n<script>alert(1)</script>");
  assert.match(html, /<h3>Evidence<\/h3>/);
  assert.match(html, /<ol>\n<li><strong>Studies<\/strong>: more<\/li>/);
  assert.match(html, /&lt;script&gt;/);
  assert.ok(!html.includes("<script>"));
});

test("math is typeset, but money and code are left alone", () => {
  const html = renderMarkdown("Divide by 2:\n\\[\nx = 0.05\n\\]\nSo \\(x\\) is it. The ball costs $0.05 and the bat $1.05.\n\n```\n\\[ not math \\]\n```");
  assert.match(html, /<div class="math"><span class="katex-display">/);
  assert.match(html, /<span class="katex">/);
  assert.match(html, /costs \$0\.05 and the bat \$1\.05/);
  assert.match(html, /<code>\\\[ not math \\\]\n<\/code>/);
});
