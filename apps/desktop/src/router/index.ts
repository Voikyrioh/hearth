import { createRouter, createWebHashHistory } from "vue-router";
import Settings from "@/pages/Settings.vue";
import Welcome from "@/pages/Welcome.vue";

// Historique par hachage : l'application est servie depuis des fichiers locaux.
export function createAppRouter() {
  return createRouter({
    history: createWebHashHistory(),
    routes: [
      { path: "/", name: "welcome", component: Welcome },
      { path: "/settings", name: "settings", component: Settings },
    ],
  });
}
