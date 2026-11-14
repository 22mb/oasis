<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, useId, watch } from "vue";
import exampleSpec from "../../../tests/fixtures/petstore.yaml?raw";
import type { Reply, Request } from "./playground.worker.ts";

const t = {
  title: "Playground",
  lead: "Edit an OpenAPI 3.0, 3.1 or 3.2 document in JSON or YAML to see the TypeScript client that oasis-gen generates from it. The generator runs in your browser, so nothing you enter is sent anywhere. The page URL holds the document, so you can share it as a link. References to other files are not supported.",
  spec: "OpenAPI document",
  api: "api.ts (generated)",
  copy: "Copy link",
  copied: "Copied",
  copyFailed: "Could not copy",
  reset: "Reset",
  loading: "Loading…",
  loadFailed: "The playground could not be loaded.",
};

const specId = useId();
const apiId = useId();

const spec = ref("");
const api = ref("");
const loaded = ref(false);
const failed = ref(false);
const error = ref("");
const copyStatus = ref("");

let unmounted = false;
let worker: Worker | undefined;
// The newest request; replies to older ones are dropped.
let requested = 0;
let generateTimer: ReturnType<typeof setTimeout> | undefined;
let hashTimer: ReturnType<typeof setTimeout> | undefined;
let copyTimer: ReturnType<typeof setTimeout> | undefined;

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

async function updateHash() {
  const text = spec.value;
  const hash = await toHash(text);
  // A later edit has scheduled its own update.
  if (unmounted || text !== spec.value) return;
  history.replaceState(history.state, "", location.pathname + location.search + hash);
}

function regenerate() {
  worker?.postMessage({ id: ++requested, text: spec.value } satisfies Request);
}

onMounted(async () => {
  try {
    worker = new Worker(new URL("./playground.worker.ts", import.meta.url), { type: "module" });
    const ready = new Promise<void>((resolve, reject) => {
      worker!.addEventListener("error", (e) => reject(e.message));
      worker!.addEventListener("message", ({ data }: MessageEvent<Reply>) => {
        if (data.type === "ready") resolve();
        else if (data.type === "failed") reject(data.message);
        else if (data.id === requested) {
          error.value = data.ok ? "" : data.text;
          if (data.ok) api.value = data.text;
          loaded.value = true;
        }
      });
    });
    const [text] = await Promise.all([fromHash(), ready]);
    if (unmounted) return;
    spec.value = text;
    regenerate();
    watch(spec, () => {
      clearTimeout(generateTimer);
      generateTimer = setTimeout(regenerate, 300);
      clearTimeout(hashTimer);
      hashTimer = setTimeout(updateHash, 300);
    });
  } catch (e) {
    failed.value = true;
    error.value = `${t.loadFailed}\n${e}`;
  }
});

onBeforeUnmount(() => {
  unmounted = true;
  worker?.terminate();
  clearTimeout(generateTimer);
  clearTimeout(hashTimer);
  clearTimeout(copyTimer);
});

function reset() {
  spec.value = exampleSpec;
}

async function copyLink() {
  try {
    // The link must hold the document as it is now, not as of the last update.
    clearTimeout(hashTimer);
    await updateHash();
    await navigator.clipboard.writeText(location.href);
    copyStatus.value = t.copied;
  } catch {
    copyStatus.value = t.copyFailed;
  }
  clearTimeout(copyTimer);
  copyTimer = setTimeout(() => (copyStatus.value = ""), 2000);
}
</script>

<template>
  <main class="playground">
    <h1>{{ t.title }}</h1>
    <p class="lead">{{ t.lead }}</p>
    <div class="actions">
      <span v-if="!loaded && !failed" class="loading">{{ t.loading }}</span>
      <!-- Outside the button: a live region inside one is not announced reliably. -->
      <span class="copy-status" role="status">{{ copyStatus }}</span>
      <button type="button" :disabled="!loaded" @click="reset">{{ t.reset }}</button>
      <button type="button" :disabled="!loaded" @click="copyLink">{{ t.copy }}</button>
    </div>
    <div class="panes">
      <section class="pane">
        <label class="bar" :for="specId">{{ t.spec }}</label>
        <textarea
          :id="specId"
          v-model="spec"
          class="code"
          :disabled="!loaded"
          spellcheck="false"
          autocapitalize="off"
          autocomplete="off"
          autocorrect="off"
          wrap="off"
        ></textarea>
      </section>
      <section class="pane">
        <h2 :id="apiId" class="bar">{{ t.api }}</h2>
        <!-- Always present, so that a message put into it is announced; polite,
             since a document is invalid most of the time while it is edited. -->
        <div class="problem" role="status">
          <pre v-if="error" class="error">{{ error }}</pre>
        </div>
        <!-- Shiki escapes the generated text. -->
        <div
          v-show="!error"
          class="code vp-code"
          role="region"
          :aria-labelledby="apiId"
          tabindex="0"
          v-html="api"
        ></div>
      </section>
    </div>
  </main>
</template>

<style scoped>
.playground {
  max-inline-size: 1440px;
  margin-inline: auto;
  padding-block: 32px 48px;
  padding-inline: 24px;
}

h1 {
  font-size: 32px;
  font-weight: 600;
  line-height: 40px;
}

.lead {
  max-inline-size: 760px;
  margin-block: 16px 0;
  color: var(--vp-c-text-2);
  line-height: 1.7;
}

.actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  margin-block: 16px 12px;
}

.loading {
  margin-inline-end: auto;
  font-size: 14px;
  color: var(--vp-c-text-2);
}

.copy-status {
  font-size: 13px;
  color: var(--vp-c-text-2);
}

button {
  padding: 2px 10px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 6px;
  font-size: 13px;
  color: var(--vp-c-text-1);
}

/* On touch screens, large enough to hit. */
@media (pointer: coarse) {
  button {
    min-block-size: 44px;
    min-inline-size: 44px;
  }
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
  grid-auto-rows: 60dvh;
  gap: 16px;
}

@media (min-width: 960px) {
  .panes {
    grid-template-columns: 1fr 1fr;
    grid-auto-rows: max(80dvh, 640px);
  }
}

.pane {
  display: flex;
  flex-direction: column;
  min-inline-size: 0;
  min-block-size: 0;
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
  /* Clips to the rounded corners without becoming a scroll container. */
  overflow: clip;
  background: var(--vp-code-block-bg);
}

.bar {
  display: flex;
  align-items: center;
  gap: 8px;
  min-block-size: 40px;
  padding-inline: 12px;
  border-block-end: 1px solid var(--vp-c-divider);
  font-size: 14px;
  font-weight: 500;
}

.code,
.error {
  flex: 1;
  min-block-size: 0;
  margin: 0;
  padding: 16px 20px;
  overflow: auto;
  font-family: var(--vp-font-family-mono);
  font-size: var(--vp-code-font-size);
  line-height: var(--vp-code-line-height);
  white-space: pre;
}

/* VitePress removes the outline of a focused textarea. The ring sits inside,
   since the pane clips what overflows it. */
.code:focus-visible {
  outline: 2px solid var(--vp-c-brand-1);
  outline-offset: -2px;
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

/* Takes the place of the generated file while there is a message. */
.problem {
  display: flex;
  min-block-size: 0;
}

.problem:not(:empty) {
  flex: 1;
}

.error {
  color: var(--vp-c-danger-1);
}
</style>
