import { readdirSync, readFileSync } from "node:fs";
import { transformerTwoslash } from "@shikijs/vitepress-twoslash";
import { defineConfig } from "vitepress";
import type { Plugin } from "vite";

const repository = "https://github.com/22mb/oasis";

// `ts twoslash` code blocks type-check against clients generated from the test
// fixtures, which `cargo test` keeps current: `./api.ts` is the petstore one,
// and a block can import `./features.ts` or `./security.ts` before `// ---cut---`.
const golden = (name: string) =>
  readFileSync(new URL(`../../tests/golden/${name}.ts`, import.meta.url), "utf8");

// VitePress puts all CSS into the one stylesheet that every page loads. Monaco's
// CSS, which only the playground needs, is instead inlined into Monaco's chunk
// and added to the document when the playground loads it.
const monacoStyles: Plugin = {
  name: "oasis:monaco-styles",
  apply: "build",
  enforce: "pre",
  resolveId: (id) => (id === "virtual:monaco-styles" ? "\0monaco-styles" : undefined),
  load: (id) =>
    id === "\0monaco-styles"
      ? `const added = new Set();
export function add(css) {
  if (typeof document === "undefined" || added.has(css)) return;
  added.add(css);
  document.head.append(Object.assign(document.createElement("style"), { textContent: css }));
}`
      : undefined,
  transform(code, id) {
    if (!id.includes("/node_modules/monaco-editor/esm/")) return;
    let count = 0;
    const result = code.replace(/^import '([^']+\.css)';$/gm, (_, path) => {
      const name = `__monacoStyle${count++}`;
      return `import ${name} from '${path}?inline'; __addMonacoStyle(${name});`;
    });
    return count ? `import { add as __addMonacoStyle } from "virtual:monaco-styles";\n${result}` : undefined;
  },
};

// Monaco's TypeScript worker bundles TypeScript 5.9. The playground checks with
// the site's TypeScript instead, the one that the twoslash blocks use, by
// replacing the modules that provide TypeScript and its lib files.
const typescriptLib = new URL("../node_modules/typescript/lib/", import.meta.url);
const monacoTypescript: Plugin = {
  name: "oasis:monaco-typescript",
  enforce: "pre",
  resolveId(id, importer) {
    if (importer?.includes("/monaco-editor/esm/vs/languages/features/typescript/") && id.startsWith("./lib/")) {
      return `\0monaco-typescript/${id.slice("./lib/".length)}`;
    }
  },
  load(id) {
    if (!id.startsWith("\0monaco-typescript/")) return;
    const libs = readdirSync(typescriptLib).filter((f) => /^lib\..*\.d\.ts$/.test(f));
    switch (id.slice("\0monaco-typescript/".length)) {
      case "typescriptServices.js":
        return `import ts from "typescript";
export const typescript = ts;
export const { EndOfLineState, IndentStyle, ScriptKind, ScriptTarget, TokenClass, createClassifier,
  createLanguageService, displayPartsToString, flattenDiagnosticMessageText } = ts;`;
      case "typescriptServicesMetadata.js":
        return `export { version as typescriptVersion } from "typescript/package.json";`;
      case "lib.js":
        return `export const libFileMap = ${JSON.stringify(
          Object.fromEntries(libs.map((f) => [f, readFileSync(new URL(f, typescriptLib), "utf8")])),
        )};`;
      case "lib.index.js":
        return `export const libFileSet = ${JSON.stringify(Object.fromEntries(libs.map((f) => [f, true])))};`;
      default:
        throw new Error(`unexpected Monaco TypeScript module ${id}`);
    }
  },
};

export default defineConfig({
  title: "oasis-gen",
  // `head` paths are not prefixed with `base`, unlike `themeConfig.logo`.
  head: [
    ["link", { rel: "icon", type: "image/svg+xml", href: "/oasis/logo.svg" }],
    ["link", { rel: "apple-touch-icon", href: "/oasis/logo-512.png" }],
    ["meta", { name: "theme-color", content: "#0B1F26" }],
    ["meta", { property: "og:type", content: "website" }],
    ["meta", { property: "og:title", content: "oasis-gen" }],
    ["meta", { property: "og:image", content: "https://22mb.github.io/oasis/ogp.png" }],
    ["meta", { name: "twitter:card", content: "summary_large_image" }],
  ],
  // Served from GitHub Pages at https://22mb.github.io/oasis/.
  base: "/oasis/",
  cleanUrls: true,
  vite: {
    plugins: [monacoStyles, monacoTypescript],
    // The TypeScript worker is bundled on its own.
    worker: { format: "es", plugins: () => [monacoTypescript] },
    // Pre-bundling would skip the plugins in `vitepress dev`.
    optimizeDeps: { exclude: ["monaco-editor"], include: ["typescript"] },
  },
  lastUpdated: false,
  markdown: {
    // The default GitHub themes leave some tokens, such as comments, below 4.5:1.
    theme: { light: "github-light-high-contrast", dark: "github-dark-high-contrast" },
    codeTransformers: [
      transformerTwoslash({
        // Show `^?` results as lines of their own instead of popups over the code.
        queryRendering: "line",
        twoslashOptions: {
          extraFiles: {
            "api.ts": golden("petstore"),
            "features.ts": golden("features"),
            "security.ts": golden("security"),
          },
          compilerOptions: {
            allowImportingTsExtensions: true,
            noEmit: true,
            exactOptionalPropertyTypes: true,
            noUncheckedIndexedAccess: true,
          },
        },
      }),
    ],
  },
  themeConfig: {
    logo: "/logo.svg",
    search: {
      provider: "local",
      options: {
        miniSearch: {
          options: {
            // The default tokenizer splits on spaces and punctuation, which leaves
            // a Japanese sentence as one token. This function is serialized to the
            // client by its source, so it must not refer to anything outside it.
            // Segments also keep `api.GET` or `oasis_gen` whole, so split those as well.
            tokenize(text: string) {
              const segmenter = new Intl.Segmenter("ja", { granularity: "word" });
              return [...segmenter.segment(text)]
                .filter((s) => s.isWordLike)
                .flatMap((s) => s.segment.split(/[\p{P}\p{S}]+/u))
                .filter(Boolean);
            },
          },
        },
        locales: {
          ja: {
            translations: {
              button: { buttonText: "検索", buttonAriaLabel: "検索" },
              modal: {
                displayDetails: "詳細を表示",
                resetButtonTitle: "検索語を消去する",
                backButtonTitle: "検索を閉じる",
                noResultsText: "一致する結果はありません：",
                footer: {
                  selectText: "選択",
                  selectKeyAriaLabel: "Enter キー",
                  navigateText: "移動",
                  navigateUpKeyAriaLabel: "上矢印キー",
                  navigateDownKeyAriaLabel: "下矢印キー",
                  closeText: "閉じる",
                  closeKeyAriaLabel: "Esc キー",
                },
              },
            },
          },
        },
      },
    },
    socialLinks: [{ icon: "github", link: repository }],
  },
  locales: {
    root: {
      label: "English",
      lang: "en",
      description: "A minimal API client generator for OpenAPI 3.2",
      themeConfig: {
        nav: [
          { text: "Guide", link: "/getting-started" },
          { text: "Reference", link: "/reference" },
          { text: "Playground", link: "/playground" },
        ],
        sidebar: [
          {
            text: "Introduction",
            items: [
              { text: "Getting started", link: "/getting-started" },
              { text: "Strict types", link: "/strict-types" },
            ],
          },
          {
            text: "Guide",
            items: [
              { text: "Calling operations", link: "/client" },
              { text: "Middleware", link: "/middleware" },
              { text: "Authentication", link: "/authentication" },
              { text: "Recipes", link: "/recipes" },
            ],
          },
          {
            text: "Reference",
            items: [
              { text: "CLI and API", link: "/reference" },
              { text: "Schemas", link: "/schemas" },
              { text: "OpenAPI support", link: "/openapi-support" },
            ],
          },
        ],
      },
    },
    ja: {
      label: "日本語",
      lang: "ja",
      description: "OpenAPI 3.2 向けの最小限の API クライアント生成ツール",
      themeConfig: {
        nav: [
          { text: "ガイド", link: "/ja/getting-started" },
          { text: "リファレンス", link: "/ja/reference" },
          { text: "プレイグラウンド", link: "/ja/playground" },
        ],
        sidebar: [
          {
            text: "はじめに",
            items: [
              { text: "クイックスタート", link: "/ja/getting-started" },
              { text: "厳密な型", link: "/ja/strict-types" },
            ],
          },
          {
            text: "ガイド",
            items: [
              { text: "操作の呼び出し", link: "/ja/client" },
              { text: "ミドルウェア", link: "/ja/middleware" },
              { text: "認証", link: "/ja/authentication" },
              { text: "レシピ", link: "/ja/recipes" },
            ],
          },
          {
            text: "リファレンス",
            items: [
              { text: "CLI と API", link: "/ja/reference" },
              { text: "スキーマ", link: "/ja/schemas" },
              { text: "OpenAPI の対応状況", link: "/ja/openapi-support" },
            ],
          },
        ],
        outline: { label: "目次" },
        docFooter: { prev: "前のページ", next: "次のページ" },
        darkModeSwitchLabel: "外観",
        lightModeSwitchTitle: "ライトモードに切り替える",
        darkModeSwitchTitle: "ダークモードに切り替える",
        sidebarMenuLabel: "メニュー",
        returnToTopLabel: "ページの先頭へ",
        langMenuLabel: "言語を切り替える",
        skipToContentLabel: "本文へ移動",
        notFound: {
          title: "ページが見つかりません",
          quote: "URL が正しいかご確認ください。",
          linkLabel: "ホームへ移動",
          linkText: "ホームへ戻る",
        },
      },
    },
  },
});
