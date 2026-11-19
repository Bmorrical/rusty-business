import { createApp } from "vue";
// The fonts ship inside the app, so the page never asks the network for them.
import "@fontsource-variable/instrument-sans";
import "@fontsource/instrument-serif";
import "@fontsource-variable/jetbrains-mono";
import "katex/dist/katex.min.css";
import App from "./App.vue";

createApp(App).mount("#app");
