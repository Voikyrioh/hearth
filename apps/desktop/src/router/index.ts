import { watch } from "vue";
import {
  createRouter,
  createWebHashHistory,
  type RouteLocationNormalized,
  type RouteLocationRaw,
  type Router,
  type RouterHistory,
} from "vue-router";
import type { MessageKey } from "@/i18n";
import ServerLayout from "@/layouts/ServerLayout.vue";
import Accounts from "@/pages/Accounts.vue";
import Audit from "@/pages/Audit.vue";
import Dashboard from "@/pages/Dashboard.vue";
import Settings from "@/pages/Settings.vue";
import Welcome from "@/pages/Welcome.vue";
import { useServersStore } from "@/stores/servers";

declare module "vue-router" {
  interface RouteMeta {
    /** Titre de la vue dans l'en-tête du serveur. */
    title?: MessageKey;
    /** Vue réservée au rôle administrateur (BR-ACCT-013). */
    adminOnly?: boolean;
  }
}

/**
 * Routes : `/welcome` (aucun serveur), `/servers/:id/{dashboard,accounts,audit}` dans le
 * gabarit du serveur, `/settings`. `/` ne rend rien : la garde l'envoie vers le premier
 * serveur ou l'accueil. Historique par hachage (l'application est servie depuis des fichiers).
 */
export function createAppRouter(history: RouterHistory = createWebHashHistory()): Router {
  const router = createRouter({
    history,
    routes: [
      { path: "/", name: "home", component: { render: () => null } },
      { path: "/welcome", name: "welcome", component: Welcome },
      {
        path: "/servers/:id",
        component: ServerLayout,
        children: [
          { path: "", redirect: { name: "dashboard" } },
          {
            path: "dashboard",
            name: "dashboard",
            component: Dashboard,
            meta: { title: "pages.dashboard" },
          },
          {
            path: "accounts",
            name: "accounts",
            component: Accounts,
            meta: { title: "pages.accounts", adminOnly: true },
          },
          {
            path: "audit",
            name: "audit",
            component: Audit,
            meta: { title: "pages.audit", adminOnly: true },
          },
        ],
      },
      { path: "/settings", name: "settings", component: Settings },
      { path: "/:pathMatch(.*)*", redirect: { name: "home" } },
    ],
  });

  let watching = false;
  router.beforeEach(async (to) => {
    const servers = useServersStore();
    try {
      await servers.load();
    } catch {
      // Pont en panne : on se comporte comme s'il n'y avait aucun serveur (jamais d'écran blanc).
    }
    if (!watching) {
      watching = true;
      // Serveur supprimé pendant qu'on le regarde : retour à l'accueil ou au premier serveur.
      watch(
        () => servers.servers,
        () => {
          const target = redirectFor(router.currentRoute.value);
          if (target) void router.replace(target);
        },
      );
    }
    return redirectFor(to) ?? true;
  });
  return router;
}

/** Où envoyer cette route si elle n'est pas valide d'après les serveurs connus ; `null` si tout va bien. */
export function redirectFor(route: RouteLocationNormalized): RouteLocationRaw | null {
  const servers = useServersStore();
  const home: RouteLocationRaw = servers.first
    ? { name: "dashboard", params: { id: servers.first.id } }
    : { name: "welcome" };
  if (route.name === "home") return home;
  if (route.name === "welcome") return servers.first ? home : null;
  if (typeof route.params.id === "string") {
    const server = servers.byId(route.params.id);
    if (!server) return home;
    if (route.matched.some((record) => record.meta.adminOnly) && server.role !== "admin") {
      return { name: "dashboard", params: { id: server.id } };
    }
  }
  return null;
}
