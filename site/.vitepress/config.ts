import { readFileSync } from "node:fs";
import { transformerTwoslash } from "@shikijs/vitepress-twoslash";
import { defineConfig } from "vitepress";

const repository = "https://github.com/22mb/oasis";

// `ts twoslash` code blocks type-check against clients generated from the test
// fixtures, which `cargo test` keeps current: `./api.ts` is the petstore one,
// and a block can import `./features.ts` or `./security.ts` before `// ---cut---`.
const golden = (name: string) =>
  readFileSync(new URL(`../../tests/golden/${name}.ts`, import.meta.url), "utf8");

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
  lastUpdated: false,
  markdown: {
    // The default GitHub themes leave some tokens, such as comments, below 4.5:1.
    theme: { light: "github-light-high-contrast", dark: "github-dark-high-contrast" },
    // VitePress loads only the languages of code blocks, but Twoslash also
    // highlights the examples in hovered JSDoc, such as TanStack Query's `tsx`.
    languages: ["tsx"],
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
