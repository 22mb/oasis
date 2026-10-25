// Monaco with only what the playground uses: the editor features, YAML and
// TypeScript highlighting, and TypeScript's language service, which checks
// main.ts against the generated api.ts. Monaco needs a browser, so the
// playground imports this module only once it is mounted.
/// <reference types="vite/client" />
import * as monaco from "monaco-editor/editor";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";
import TypescriptWorker from "monaco-editor/languages/features/typescript/ts.worker?worker";
import "monaco-editor/features/register.all";
import "monaco-editor/languages/definitions/typescript/register";
import "monaco-editor/languages/definitions/yaml/register";
import {
  ModuleKind,
  type ModuleResolutionKind,
  type ScriptTarget,
  typescriptDefaults,
  typescriptVersion,
} from "monaco-editor/languages/features/typescript/register";

export { typescriptVersion };

// Monaco takes every worker from here once this is defined.
(globalThis as { MonacoEnvironment?: object }).MonacoEnvironment = {
  getWorker: (_: string, label: string) =>
    label === "typescript" ? new TypescriptWorker() : new EditorWorker(),
};

// What the generated file requires (see reference.md#requirements), with the
// options the site's twoslash blocks check against. `lib` follows `target`:
// ES2022 and the DOM.
typescriptDefaults.setCompilerOptions({
  strict: true,
  // ES2022 and Bundler, which Monaco's enums lack; TypeScript takes the numbers.
  target: 9 as ScriptTarget,
  module: ModuleKind.ESNext,
  moduleResolution: 100 as ModuleResolutionKind,
  allowImportingTsExtensions: true,
  noEmit: true,
  exactOptionalPropertyTypes: true,
  noUncheckedIndexedAccess: true,
});
// Check api.ts and main.ts together even before either has been shown.
typescriptDefaults.setEagerModelSync(true);

// The panes paint the site's code block background behind the editors.
for (const [name, base] of [
  ["oasis-light", "vs"],
  ["oasis-dark", "vs-dark"],
] as const) {
  monaco.editor.defineTheme(name, {
    base,
    inherit: true,
    rules: [],
    colors: { "editor.background": "#00000000" },
  });
}

export interface Editors {
  spec: monaco.editor.ITextModel;
  main: monaco.editor.ITextModel;
  api: monaco.editor.ITextModel;
  dispose(): void;
}

export function createEditors(
  elements: { spec: HTMLElement; main: HTMLElement; api: HTMLElement },
  values: { spec: string; main: string },
): Editors {
  const models = {
    spec: monaco.editor.createModel(values.spec, "yaml"),
    main: monaco.editor.createModel(values.main, "typescript", monaco.Uri.parse("file:///main.ts")),
    api: monaco.editor.createModel("", "typescript", monaco.Uri.parse("file:///api.ts")),
  };
  const style = getComputedStyle(document.documentElement);
  const options = {
    automaticLayout: true,
    fontFamily: style.getPropertyValue("--vp-font-family-mono"),
    fontSize: 13,
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
    // Transparent like the editor, so it would show the text under it.
    stickyScroll: { enabled: false },
    tabSize: 2,
  };
  const editors = [
    monaco.editor.create(elements.spec, { ...options, model: models.spec }),
    monaco.editor.create(elements.main, { ...options, model: models.main }),
    monaco.editor.create(elements.api, { ...options, model: models.api, readOnly: true }),
  ];

  const setTheme = () =>
    monaco.editor.setTheme(
      document.documentElement.classList.contains("dark") ? "oasis-dark" : "oasis-light",
    );
  setTheme();
  const observer = new MutationObserver(setTheme);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });

  return {
    ...models,
    dispose() {
      observer.disconnect();
      for (const editor of editors) editor.dispose();
      for (const model of Object.values(models)) model.dispose();
    },
  };
}
