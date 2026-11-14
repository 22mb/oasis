// Generates and highlights off the main thread: highlighting the generated
// file takes a few hundred milliseconds, which would block typing.
import type { HighlighterCore } from "shiki/core";
// Built by `npm run wasm` from playground/ at the repository root.
import wasmUrl from "./oasis.wasm?url";

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

// The site's code block themes, with the JavaScript regex engine instead of
// Oniguruma's WebAssembly.
async function createHighlighter(): Promise<HighlighterCore> {
  const [{ createHighlighterCore }, { createJavaScriptRegexEngine }] = await Promise.all([
    import("shiki/core"),
    import("shiki/engine/javascript"),
  ]);
  return createHighlighterCore({
    engine: createJavaScriptRegexEngine(),
    langs: [import("shiki/langs/typescript.mjs")],
    themes: [
      import("shiki/themes/github-light-high-contrast.mjs"),
      import("shiki/themes/github-dark-high-contrast.mjs"),
    ],
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
    // VitePress colors `.vp-code` spans from these variables, by the color scheme.
    const html = highlighter.codeToHtml(result.text, {
      lang: "typescript",
      themes: { light: "github-light-high-contrast", dark: "github-dark-high-contrast" },
      defaultColor: false,
      // The pane that scrolls the code is the tab stop.
      tabindex: false,
    });
    scope.postMessage({ type: "result", id, ok: true, text: html });
  } catch (e) {
    scope.postMessage({ type: "result", id, ok: false, text: String(e) });
  }
});
