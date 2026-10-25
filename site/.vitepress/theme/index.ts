import TwoslashFloatingVue from "@shikijs/vitepress-twoslash/client";
import "@shikijs/vitepress-twoslash/style.css";
import type { Theme } from "vitepress";
import DefaultTheme from "vitepress/theme";
import Playground from "./Playground.vue";
import "./style.css";

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.use(TwoslashFloatingVue);
    app.component("Playground", Playground);
  },
} satisfies Theme;
