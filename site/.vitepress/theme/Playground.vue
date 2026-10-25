<script setup lang="ts">
import { useData } from "vitepress";
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import exampleSpec from "../../../tests/fixtures/petstore.yaml?raw";
import type { Editors } from "./monaco";
// Built by `npm run wasm` from playground/ at the repository root.
import wasmUrl from "./oasis.wasm?url";

const exampleMain = `import { createClient } from "./api.ts";

const api = createClient();

const res = await api.GET("/pets/{petId}", { path: { petId: 1 } });
switch (res.status) {
  case 200:
    console.log(res.data.name, res.data.kind);
    break;
  case 404:
    console.log(res.data.title);
    break;
  default:
    console.error(res.status, res.data);
}
`;

const labels = {
  en: {
    title: "Playground",
    lead: "Edit an OpenAPI 3.0, 3.1 or 3.2 document in JSON or YAML to see the TypeScript client that oasis-gen generates from it. Write code that uses the client in main.ts to see how it type-checks. The generator and TypeScript run in your browser, so nothing you enter is sent anywhere. The page URL holds the document and main.ts, so you can share them as a link. References to other files are not supported.",
    spec: "OpenAPI document",
    api: "api.ts (generated)",
    copy: "Copy link",
    copied: "Copied",
    reset: "Reset",
    loading: "Loading…",
  },
  ja: {
    title: "プレイグラウンド",
    lead: "JSON または YAML の OpenAPI 3.0・3.1・3.2 ドキュメントを編集すると、oasis-gen が生成する TypeScript クライアントを確認できます。main.ts にクライアントを使うコードを書くと、その型チェックの結果も確認できます。生成ツールと TypeScript はブラウザー内で動作するため、入力した内容が外部に送信されることはありません。ページの URL にはドキュメントと main.ts の内容が含まれており、そのままリンクとして共有できます。外部ファイルへの参照には対応していません。",
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

const specPane = ref<HTMLElement>();
const mainPane = ref<HTMLElement>();
const apiPane = ref<HTMLElement>();
const loaded = ref(false);
const error = ref("");
const copied = ref(false);
const typescript = ref("");

let editors: Editors | undefined;
let unmounted = false;

// The share link carries each file that differs from the example as
// `#spec=…&main=…`: its UTF-8, compressed with raw DEFLATE and encoded as
// unpadded base64url.
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

async function fromHash(key: string, example: string): Promise<string> {
  const code = new URLSearchParams(location.hash.slice(1)).get(key);
  if (code === null) return example;
  try {
    return await decode(code);
  } catch {
    // A truncated or edited link: keep the example.
    return example;
  }
}

async function toHash(files: [key: string, text: string, example: string][]): Promise<string> {
  const params = new URLSearchParams();
  for (const [key, text, example] of files) {
    if (text !== example) params.set(key, await encode(text));
  }
  const hash = params.toString();
  return hash ? `#${hash}` : "";
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
  const [spec, main, module, { createEditors, typescriptVersion }] = await Promise.all([
    fromHash("spec", exampleSpec),
    fromHash("main", exampleMain),
    WebAssembly.compileStreaming(fetch(wasmUrl)),
    import("./monaco"),
  ]);
  if (unmounted) return;
  typescript.value = `TypeScript ${typescriptVersion}`;
  loaded.value = true;
  const current = createEditors(
    { spec: specPane.value!, main: mainPane.value!, api: apiPane.value! },
    { spec, main },
  );
  editors = current;

  const regenerate = () => {
    try {
      const result = generate(module, current.spec.getValue());
      error.value = result.ok ? "" : result.text;
      // On an error, main.ts keeps checking against the last generated client.
      if (result.ok) current.api.setValue(result.text);
    } catch (e) {
      error.value = String(e);
    }
  };
  const updateHash = async () => {
    const spec = current.spec.getValue();
    const main = current.main.getValue();
    const hash = await toHash([
      ["spec", spec, exampleSpec],
      ["main", main, exampleMain],
    ]);
    // A later edit has scheduled its own update.
    if (unmounted || spec !== current.spec.getValue() || main !== current.main.getValue()) return;
    history.replaceState(history.state, "", location.pathname + location.search + hash);
  };

  let generateTimer: ReturnType<typeof setTimeout> | undefined;
  let hashTimer: ReturnType<typeof setTimeout> | undefined;
  const scheduleHash = () => {
    clearTimeout(hashTimer);
    hashTimer = setTimeout(updateHash, 300);
  };
  current.spec.onDidChangeContent(() => {
    clearTimeout(generateTimer);
    generateTimer = setTimeout(regenerate, 300);
    scheduleHash();
  });
  current.main.onDidChangeContent(scheduleHash);
  regenerate();
});

onBeforeUnmount(() => {
  unmounted = true;
  editors?.dispose();
});

function reset() {
  editors?.spec.setValue(exampleSpec);
  editors?.main.setValue(exampleMain);
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
      <section class="pane spec">
        <div class="bar">{{ t.spec }}</div>
        <div ref="specPane" class="editor"></div>
      </section>
      <section class="pane main">
        <div class="bar">
          main.ts
          <span class="note">{{ typescript }}</span>
        </div>
        <div ref="mainPane" class="editor"></div>
      </section>
      <section class="pane api">
        <div class="bar">{{ t.api }}</div>
        <pre v-if="error" class="error" role="alert">{{ error }}</pre>
        <div v-show="!error" ref="apiPane" class="editor"></div>
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

/* One column of the document, main.ts and api.ts; on wide screens, the two
   inputs on the left and the generated file on the right. */
.panes {
  display: grid;
  grid-template-columns: 1fr;
  grid-auto-rows: 60vh;
  gap: 16px;
}

@media (min-width: 960px) {
  .panes {
    grid-template-columns: 1fr 1fr;
    grid-template-rows: 3fr 2fr;
    height: max(80vh, 640px);
  }

  .api {
    grid-column: 2;
    grid-row: 1 / 3;
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

.note {
  margin-left: auto;
  font-weight: 400;
  color: var(--vp-c-text-2);
}

.editor,
.error {
  flex: 1;
  min-height: 0;
}

.error {
  margin: 0;
  padding: 16px 20px;
  overflow: auto;
  font-family: var(--vp-font-family-mono);
  font-size: var(--vp-code-font-size);
  line-height: var(--vp-code-line-height);
  white-space: pre;
  color: var(--vp-c-danger-1);
}
</style>
