import { createPinia } from "pinia";
import { createApp } from "vue";
import App from "./App.vue";
import { vNeedsLink } from "./directives/needsLink";
import { installErrorHandlers } from "./errors/install";
import { createAppRouter } from "./router";
import "./styles/fonts.css";
import "./styles/tokens.css";
import "./styles/base.css";

const app = createApp(App);
installErrorHandlers(app);
app.directive("needs-link", vNeedsLink).use(createPinia()).use(createAppRouter()).mount("#app");
