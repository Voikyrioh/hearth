import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { flushPromises } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createApp, defineComponent, h } from "vue";
import App from "@/App.vue";
import { installErrorHandlers } from "@/errors/install";
import { resetReportRate } from "@/errors/report";
import { mountContext } from "@/test/mount";

// Un composant de la COQUILLE elle-même (barre, navigation, en-tête) dont le rendu plante :
// Vue le remplace par un commentaire vide (jamais d'écran blanc), le reste de la coquille et la
// page restent utilisables, l'erreur est notifiée et journalisée.
const crash = vi.hoisted(() => ({ target: "" }));

async function wrapped(path: string, name: string, props: string[]) {
  const original = (await vi.importActual<{ default: object }>(path)).default;
  return {
    default: defineComponent({
      name,
      props,
      setup(p, { slots }) {
        if (crash.target === name) throw new Error(`${name} cassé`);
        return () => h(original, p, slots);
      },
    }),
  };
}

vi.mock("@/components/organisms/ServerRail.vue", () =>
  wrapped("@/components/organisms/ServerRail.vue", "ServerRail", []),
);
vi.mock("@/components/organisms/ServerNav.vue", () =>
  wrapped("@/components/organisms/ServerNav.vue", "ServerNav", ["server"]),
);
vi.mock("@/components/organisms/AppHeader.vue", () =>
  wrapped("@/components/organisms/AppHeader.vue", "AppHeader", ["title", "serverId"]),
);

const ipc: { cmd: string; args?: Record<string, string> }[] = [];

beforeEach(() => {
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  });
  ipc.length = 0;
  resetReportRate();
  mockIPC((cmd, args) => {
    ipc.push({ cmd, args: args as Record<string, string> });
  });
  vi.spyOn(console, "warn").mockImplementation(() => {});
});
afterEach(() => {
  crash.target = "";
  clearMocks();
  vi.useRealTimers();
});

describe("a render error inside the shell itself", () => {
  // Montage identique à `main.ts` : vraie application, gestionnaires globaux installés.
  async function boot() {
    const ctx = await mountContext();
    await ctx.router.push("/servers/forge/dashboard");
    const root = document.createElement("div");
    document.body.appendChild(root);
    const app = createApp(App);
    installErrorHandlers(app);
    app.use(ctx.pinia).use(ctx.router).mount(root);
    await flushPromises();
    return { ...ctx, root, app };
  }

  it.each([
    [
      "ServerRail",
      'nav[aria-label="Serveurs"]',
      ['nav[aria-label="Navigation du serveur"]', ".head [role=status]"],
    ],
    [
      "ServerNav",
      'nav[aria-label="Navigation du serveur"]',
      ['nav[aria-label="Serveurs"]', ".head [role=status]"],
    ],
    [
      "AppHeader",
      ".head",
      ['nav[aria-label="Serveurs"]', 'nav[aria-label="Navigation du serveur"]'],
    ],
  ])(
    "%s crashing: no blank screen, the rest stays usable, error notified and logged",
    async (name, gone, kept) => {
      crash.target = name;
      const { root, app, bridge } = await boot();
      expect(root.querySelector(gone)).toBeNull();
      for (const selector of kept) expect(root.querySelector(selector), selector).not.toBeNull();
      // La page est affichée, l'application répond encore.
      expect(root.textContent).toContain("Bientôt disponible");
      bridge.setState("forge", "offline");
      await flushPromises();
      if (name !== "AppHeader") {
        expect(root.querySelector(".head [role=status]")?.textContent).toBe("Hors ligne");
      }
      expect(root.querySelector(".banner")).not.toBeNull();
      // Erreur notifiée discrètement (pas de repli plein écran) et journalisée.
      expect(root.querySelector('[role="alert"]')).toBeNull();
      expect(root.querySelector(".toast")?.textContent).toContain("problème est survenu");
      const logged = ipc.find((call) => call.cmd === "log_frontend_error");
      expect(logged?.args?.message).toContain(`${name} cassé`);
      expect(logged?.args?.source).toContain("vue:");
      app.unmount();
      root.remove();
    },
  );
});
