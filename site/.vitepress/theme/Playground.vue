<script setup lang="ts">
import type { HighlighterCore } from "shiki/core";
import { useData } from "vitepress";
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import exampleSpec from "../../../tests/fixtures/petstore.yaml?raw";
// Built by `npm run wasm` from playground/ at the repository root.
import wasmUrl from "./oasis.wasm?url";

const labels = {
  en: {
    title: "Playground",
    lead: "Edit an OpenAPI 3.0, 3.1 or 3.2 document in JSON or YAML to see the TypeScript client that oasis-gen generates from it. The generator runs in your browser, so nothing you enter is sent anywhere. The page URL holds the document, so you can share it as a link. References to other files are not supported.",
    spec: "OpenAPI document",
    api: "api.ts (generated)",
    copy: "Copy link",
    copied: "Copied",
    reset: "Reset",
    loading: "Loading…",
  },
  ja: {
    title: "プレイグラウンド",
    lead: "JSON または YAML の OpenAPI 3.0・3.1・3.2 ドキュメントを編集すると、oasis-gen が生成する TypeScript クライアントを確認できます。生成ツールはブラウザー内で動作するため、入力した内容が外部に送信されることはありません。ページの URL にはドキュメントの内容が含まれており、そのままリンクとして共有できます。外部ファイルへの参照には対応していません。",
    spec: "OpenAPI ドキュメント",
    api: "api.ts（生成結果）",
    copy: "リンクをコピー",
    copied: "コピーしました",
    reset: "リセット",
    loading: "読み込み中…",
  },
};

const { lang } = useData();
const t = computed(() => (lang.value === "ja" ? labels.ja : labels.en));

const spec = ref("");
const api = ref("");
const loaded = ref(false);
const error = ref("");
const copied = ref(false);

let unmounted = false;
let generateTimer: ReturnType<typeof setTimeout> | undefined;
let hashTimer: ReturnType<typeof setTimeout> | undefined;

// The share link carries the document, if it differs from the example, as
// `#spec=…`: its UTF-8, compressed with raw DEFLATE and encoded as unpadded
// base64url.
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

async function fromHash(): Promise<string> {
  const code = new URLSearchParams(location.hash.slice(1)).get("spec");
  if (code === null) return exampleSpec;
  try {
    return await decode(code);
  } catch {
    // A truncated or edited link: keep the example.
    return exampleSpec;
  }
}

async function toHash(text: string): Promise<string> {
  return text === exampleSpec ? "" : `#${new URLSearchParams({ spec: await encode(text) })}`;
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

onMounted(async () => {
  const [text, module, highlighter] = await Promise.all([
    fromHash(),
    WebAssembly.compileStreaming(fetch(wasmUrl)),
    createHighlighter(),
  ]);
  if (unmounted) return;

  const regenerate = () => {
    try {
      const result = generate(module, spec.value);
      error.value = result.ok ? "" : result.text;
      if (result.ok) {
        // VitePress colors `.vp-code` spans from these variables, by the color scheme.
        api.value = highlighter.codeToHtml(result.text, {
          lang: "typescript",
          themes: { light: "github-light-high-contrast", dark: "github-dark-high-contrast" },
          defaultColor: false,
        });
      }
    } catch (e) {
      error.value = String(e);
    }
  };
  const updateHash = async () => {
    const text = spec.value;
    const hash = await toHash(text);
    // A later edit has scheduled its own update.
    if (unmounted || text !== spec.value) return;
    history.replaceState(history.state, "", location.pathname + location.search + hash);
  };

  spec.value = text;
  regenerate();
  loaded.value = true;
  watch(spec, () => {
    clearTimeout(generateTimer);
    generateTimer = setTimeout(regenerate, 300);
    clearTimeout(hashTimer);
    hashTimer = setTimeout(updateHash, 300);
  });
});

onBeforeUnmount(() => {
  unmounted = true;
  clearTimeout(generateTimer);
  clearTimeout(hashTimer);
});

function reset() {
  spec.value = exampleSpec;
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
    <div class="actions">
      <span v-if="!loaded" class="loading">{{ t.loading }}</span>
      <button type="button" :disabled="!loaded" @click="reset">{{ t.reset }}</button>
      <button type="button" :disabled="!loaded" @click="copyLink">
        {{ copied ? t.copied : t.copy }}
      </button>
    </div>
    <div class="panes">
      <section class="pane">
        <div class="bar">{{ t.spec }}</div>
        <textarea
          v-model="spec"
          class="code"
          :aria-label="t.spec"
          :disabled="!loaded"
          spellcheck="false"
          autocapitalize="off"
          autocomplete="off"
          wrap="off"
        ></textarea>
      </section>
      <section class="pane">
        <div class="bar">{{ t.api }}</div>
        <pre v-if="error" class="error" role="alert">{{ error }}</pre>
        <!-- Shiki escapes the generated text. -->
        <div v-show="!error" class="code vp-code" v-html="api"></div>
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
  margin: 16px 0 0;
  color: var(--vp-c-text-2);
  line-height: 1.7;
}

.actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  margin: 16px 0 12px;
}

.loading {
  margin-right: auto;
  font-size: 14px;
  color: var(--vp-c-text-2);
}

button {
  padding: 2px 10px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 6px;
  font-size: 13px;
  color: var(--vp-c-text-1);
}

button:hover:not(:disabled) {
  border-color: var(--vp-c-brand-1);
  color: var(--vp-c-brand-1);
}

button:disabled {
  color: var(--vp-c-text-3);
}

/* The document above the generated file; on wide screens, side by side. */
.panes {
  display: grid;
  grid-template-columns: 1fr;
  grid-auto-rows: 60vh;
  gap: 16px;
}

@media (min-width: 960px) {
  .panes {
    grid-template-columns: 1fr 1fr;
    grid-auto-rows: max(80vh, 640px);
  }
}

.pane {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
  overflow: hidden;
  background: var(--vp-code-block-bg);
}

.bar {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 40px;
  padding: 0 12px;
  border-bottom: 1px solid var(--vp-c-divider);
  font-size: 14px;
  font-weight: 500;
}

.code,
.error {
  margin: 0;
  padding: 16px 20px;
  overflow: auto;
  font-family: var(--vp-font-family-mono);
  font-size: var(--vp-code-font-size);
  line-height: var(--vp-code-line-height);
  white-space: pre;
}

.code,
.error {
  flex: 1;
  min-height: 0;
}

textarea.code {
  border: 0;
  resize: none;
  color: var(--vp-c-text-1);
  background: transparent;
  tab-size: 2;
}

.code :deep(pre) {
  margin: 0;
  background: transparent !important;
}

.error {
  color: var(--vp-c-danger-1);
}
</style>
