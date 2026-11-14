// Generates and highlights off the main thread: highlighting the generated
// file takes a few hundred milliseconds, which would block typing.
import type { HighlighterCore } from "shiki/core";

// Built by `npm run wasm` from playground/ at the repository root.
const wasmUrl = new URL("./oasis.wasm", import.meta.url);

export type Request = { id: number; text: string };
export type Reply =
  | { type: "ready" }
  | { type: "failed"; message: string }
  // `text` is the highlighted file as HTML, or the generator's error message.
  | { type: "result"; id: number; ok: boolean; text: string };

const scope = self as unknown as {
  postMessage(reply: Reply): void;
  addEventListener(type: "message", listener: (event: MessageEvent<Request>) => void): void;
};

// Each run gets a fresh instance, so a trap in one cannot affect the next.
function generate(module: WebAssembly.Module, text: string): { ok: boolean; text: string } {
  const wasm = new WebAssembly.Instance(module).exports as {
    memory: WebAssembly.Memory;
    input(len: number): number;
    generate(): number;
    output(): number;
    output_len(): number;
  };
  const bytes = new TextEncoder().encode(text);
  // Read `memory.buffer` after each call: growing the memory replaces it.
  const at = wasm.input(bytes.length);
  new Uint8Array(wasm.memory.buffer, at, bytes.length).set(bytes);
  const ok = wasm.generate() === 1;
  const ptr = wasm.output();
  const len = wasm.output_len();
  return { ok, text: new TextDecoder().decode(new Uint8Array(wasm.memory.buffer, ptr, len)) };
}

// Colors come from the `--shiki-*` variables of the site's code blocks, so the
// output follows the color scheme. The JavaScript regex engine stands in for
// Oniguruma's WebAssembly.
async function createHighlighter(): Promise<HighlighterCore> {
  const [{ createHighlighterCore, createCssVariablesTheme }, { createJavaScriptRegexEngine }] =
    await Promise.all([import("shiki/core"), import("shiki/engine/javascript")]);
  return createHighlighterCore({
    engine: createJavaScriptRegexEngine(),
    langs: [import("shiki/langs/typescript.mjs")],
    themes: [createCssVariablesTheme({ name: "css-variables", variablePrefix: "--shiki-" })],
  });
}

const loading = Promise.all([WebAssembly.compileStreaming(fetch(wasmUrl)), createHighlighter()]);
loading.then(
  () => scope.postMessage({ type: "ready" }),
  (e) => scope.postMessage({ type: "failed", message: String(e) }),
);

scope.addEventListener("message", async ({ data: { id, text } }) => {
  try {
    const [module, highlighter] = await loading;
    const result = generate(module, text);
    if (!result.ok) return scope.postMessage({ type: "result", id, ok: false, text: result.text });
    const html = highlighter.codeToHtml(result.text, {
      lang: "typescript",
      theme: "css-variables",
      // The pane that scrolls the code is the tab stop.
      tabindex: false,
    });
    scope.postMessage({ type: "result", id, ok: true, text: html });
  } catch (e) {
    scope.postMessage({ type: "result", id, ok: false, text: String(e) });
  }
});
