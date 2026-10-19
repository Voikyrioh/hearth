import { createPinia } from "pinia";
import { createApp } from "vue";
import App from "./App.vue";
import { installInputModality } from "./a11y/inputModality";
import { installErrorHandlers } from "./errors/install";
import { createAppRouter } from "./router";
import "./styles/fonts.css";
import "./styles/tokens.css";
import "./styles/base.css";

const app = createApp(App);
installErrorHandlers(app);
installInputModality(document);
app.use(createPinia()).use(createAppRouter()).mount("#app");
