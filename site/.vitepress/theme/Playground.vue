<script setup lang="ts">
import type { HighlighterCore } from "shiki/core";
import { useData } from "vitepress";
import { computed, onMounted, ref, watch } from "vue";
import example from "../../../tests/fixtures/petstore.yaml?raw";
// Built by `npm run wasm` from playground/ at the repository root.
import wasmUrl from "./oasis.wasm?url";

const labels = {
  en: {
    title: "Playground",
    lead: "Edit an OpenAPI 3.0, 3.1 or 3.2 document in JSON or YAML to see the TypeScript client that oasis-gen generates from it. The generator runs in your browser as WebAssembly, so the document is not sent anywhere. The page URL holds the document, so you can share it as a link. References to other files are not supported.",
    input: "OpenAPI document",
    output: "Generated client",
    copy: "Copy link",
    copied: "Copied",
    reset: "Reset",
    loading: "Loading…",
  },
  ja: {
    title: "プレイグラウンド",
    lead: "JSON または YAML の OpenAPI 3.0・3.1・3.2 ドキュメントを編集すると、oasis-gen が生成する TypeScript クライアントを確認できます。生成ツールは WebAssembly としてブラウザー内で動作するため、ドキュメントが外部に送信されることはありません。ページの URL にはドキュメントが含まれており、そのままリンクとして共有できます。外部ファイルへの参照には対応していません。",
    input: "OpenAPI ドキュメント",
    output: "生成されたクライアント",
    copy: "リンクをコピー",
    copied: "コピーしました",
    reset: "リセット",
    loading: "読み込み中…",
  },
};

const { lang } = useData();
const t = computed(() => (lang.value === "ja" ? labels.ja : labels.en));

const source = ref(example);
const html = ref("");
const error = ref("");
const copied = ref(false);

// The share link carries the document as `#code=` followed by its UTF-8,
// compressed with raw DEFLATE and encoded as unpadded base64url.
const prefix = "#code=";

async function encode(text: string): Promise<string> {
  const stream = new Blob([text]).stream().pipeThrough(new CompressionStream("deflate-raw"));
  const bytes = new Uint8Array(await new Response(stream).arrayBuffer());
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function decode(code: string): Promise<string> {
  const binary = atob(code.replace(/-/g, "+").replace(/_/g, "/"));
  const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0));
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
  return new Response(stream).text();
}

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

onMounted(async () => {
  if (location.hash.startsWith(prefix)) {
    try {
      source.value = await decode(location.hash.slice(prefix.length));
    } catch {
      // A truncated or edited link: keep the example.
    }
  }
  const [module, highlighter] = await Promise.all([
    WebAssembly.compileStreaming(fetch(wasmUrl)),
    loadHighlighter(),
  ]);

  let timer: ReturnType<typeof setTimeout> | undefined;
  const update = async () => {
    const text = source.value;
    try {
      const result = generate(module, text);
      error.value = result.ok ? "" : result.text;
      if (result.ok) html.value = highlight(highlighter, result.text);
    } catch (e) {
      error.value = String(e);
    }
    const hash = text === example ? "" : prefix + (await encode(text));
    if (text === source.value) {
      history.replaceState(history.state, "", location.pathname + location.search + hash);
    }
  };
  watch(source, () => {
    clearTimeout(timer);
    timer = setTimeout(update, 300);
  });
  await update();
});

async function loadHighlighter(): Promise<HighlighterCore> {
  const [{ createHighlighterCore }, { createJavaScriptRegexEngine }] = await Promise.all([
    import("shiki/core"),
    import("shiki/engine/javascript"),
  ]);
  return createHighlighterCore({
    // The themes of the site's code blocks, in `markdown.theme` of config.ts.
    themes: [
      import("shiki/themes/github-light-high-contrast.mjs"),
      import("shiki/themes/github-dark-high-contrast.mjs"),
    ],
    langs: [import("shiki/langs/typescript.mjs")],
    engine: createJavaScriptRegexEngine(),
  });
}

// Colors as `--shiki-light` and `--shiki-dark`, which the default theme's
// `.vp-code` styles pick from by the color mode.
function highlight(highlighter: HighlighterCore, code: string): string {
  return highlighter.codeToHtml(code, {
    lang: "typescript",
    themes: { light: "github-light-high-contrast", dark: "github-dark-high-contrast" },
    defaultColor: false,
    transformers: [
      {
        pre(node) {
          this.addClassToHast(node, "vp-code");
        },
      },
    ],
  });
}

async function copyLink() {
  await navigator.clipboard.writeText(location.href);
  copied.value = true;
  setTimeout(() => (copied.value = false), 2000);
}
</script>

<template>
  <div class="playground">
    <h1>{{ t.title }}</h1>
    <p class="lead">{{ t.lead }}</p>
    <div class="panes">
      <section class="pane">
        <div class="bar">
          <label for="playground-input">{{ t.input }}</label>
          <button type="button" @click="source = example">{{ t.reset }}</button>
          <button type="button" @click="copyLink">{{ copied ? t.copied : t.copy }}</button>
        </div>
        <textarea
          id="playground-input"
          v-model="source"
          spellcheck="false"
          autocapitalize="off"
          autocomplete="off"
          wrap="off"
        ></textarea>
      </section>
      <section class="pane">
        <div class="bar">
          <span>{{ t.output }}</span>
        </div>
        <pre v-if="error" class="error" role="alert">{{ error }}</pre>
        <div v-else-if="html" class="output" v-html="html"></div>
        <p v-else class="loading">{{ t.loading }}</p>
      </section>
    </div>
  </div>
</template>

<style scoped>
.playground {
  max-width: 1440px;
  margin: 0 auto;
  padding: 32px 24px 48px;
}

h1 {
  font-size: 32px;
  font-weight: 600;
  line-height: 40px;
}

.lead {
  max-width: 760px;
  margin: 16px 0 24px;
  color: var(--vp-c-text-2);
  line-height: 1.7;
}

.panes {
  display: grid;
  grid-template-columns: 1fr;
  gap: 16px;
}

@media (min-width: 960px) {
  .panes {
    grid-template-columns: 1fr 1fr;
  }
}

.pane {
  display: flex;
  flex-direction: column;
  min-width: 0;
  height: 70vh;
  min-height: 400px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
  overflow: hidden;
  background: var(--vp-code-block-bg);
}

.bar {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 48px;
  padding: 0 12px;
  border-bottom: 1px solid var(--vp-c-divider);
  font-size: 14px;
  font-weight: 500;
}

.bar > :first-child {
  margin-right: auto;
}

button {
  padding: 2px 10px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 6px;
  font-size: 13px;
  color: var(--vp-c-text-1);
}

button:hover {
  border-color: var(--vp-c-brand-1);
  color: var(--vp-c-brand-1);
}

textarea,
.output,
.error,
.loading {
  flex: 1;
  margin: 0;
  overflow: auto;
}

textarea,
.error,
.output :deep(pre) {
  padding: 16px 20px;
  font-family: var(--vp-font-family-mono);
  font-size: var(--vp-code-font-size);
  line-height: var(--vp-code-line-height);
  resize: none;
  white-space: pre;
  color: var(--vp-c-text-1);
  background: transparent;
}

.error {
  color: var(--vp-c-danger-1);
}

.loading {
  padding: 16px 20px;
  color: var(--vp-c-text-2);
}

.output :deep(pre) {
  margin: 0;
}
</style>
