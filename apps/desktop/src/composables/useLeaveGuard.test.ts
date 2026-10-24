import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h } from "vue";
import { createMemoryHistory, createRouter, RouterView } from "vue-router";
import { useLeaveGuard } from "./useLeaveGuard";

// La mécanique de sortie partagée par le formulaire de déploiement et l'installation d'une fiche : une saisie ne se perd
// jamais sans question, un lancement laisse partir, et l'autorisation de sortie retombe si la navigation échoue.

afterEach(() => {
  document.body.innerHTML = "";
});

async function setup(initial: { dirty: boolean; launching?: boolean }) {
  const state = { dirty: initial.dirty, launching: initial.launching ?? false };
  const wipe = vi.fn(() => {
    state.dirty = false;
  });
  let guard!: ReturnType<typeof useLeaveGuard>;
  const Form = defineComponent({
    setup() {
      guard = useLeaveGuard({
        dirty: () => state.dirty,
        launching: () => state.launching,
        home: () => ({ name: "home" }),
        wipe,
      });
      return () => h("div");
    },
  });
  const blank = { render: () => h("div") };
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", name: "home", component: blank },
      { path: "/form", name: "form", component: Form },
      { path: "/other", name: "other", component: blank },
    ],
  });
  await router.push("/form");
  await router.isReady();
  const wrapper = mount({ render: () => h(RouterView) }, { global: { plugins: [router] } });
  await flushPromises();
  return { router, state, wipe, guard, wrapper };
}

function unloadAsked(): boolean {
  const event = new Event("beforeunload", { cancelable: true });
  window.dispatchEvent(event);
  return event.defaultPrevented;
}

describe("useLeaveGuard", () => {
  it("laisse partir sans question quand rien n'est saisi", async () => {
    const { router, guard, wrapper } = await setup({ dirty: false });
    await router.push("/other");
    expect(router.currentRoute.value.name).toBe("other");
    expect(guard.abandonOpen.value).toBe(false);
    wrapper.unmount();
  });

  it("retient une saisie en cours et pose la question, pour le routeur comme pour le rechargement", async () => {
    const { router, guard, wrapper } = await setup({ dirty: true });
    await router.push("/other");
    expect(router.currentRoute.value.name).toBe("form");
    expect(guard.abandonOpen.value).toBe(true);
    expect(unloadAsked()).toBe(true);
    // « Continuer la saisie » : la question se ferme, la saisie reste.
    guard.keepTyping();
    expect(guard.abandonOpen.value).toBe(false);
    wrapper.unmount();
  });

  it("laisse partir après la confirmation : saisie effacée, direction la destination demandée", async () => {
    const { router, state, wipe, guard, wrapper } = await setup({ dirty: true });
    await router.push("/other");
    expect(guard.abandonOpen.value).toBe(true);
    guard.abandon();
    await flushPromises();
    expect(wipe).toHaveBeenCalledOnce();
    expect(state.dirty).toBe(false);
    expect(guard.abandonOpen.value).toBe(false);
    expect(router.currentRoute.value.name).toBe("other");
    wrapper.unmount();
  });

  it("« Fermer » avec une saisie pose la question, sans saisie va à l'accueil", async () => {
    const { router, state, guard, wrapper } = await setup({ dirty: true });
    guard.requestClose();
    expect(guard.abandonOpen.value).toBe(true);
    expect(router.currentRoute.value.name).toBe("form");
    guard.keepTyping();
    state.dirty = false;
    guard.requestClose();
    await flushPromises();
    expect(router.currentRoute.value.name).toBe("home");
    wrapper.unmount();
  });

  it("ne demande rien une fois l'opération lancée", async () => {
    const { router, guard, wrapper } = await setup({ dirty: true, launching: true });
    expect(unloadAsked()).toBe(false);
    await router.push("/other");
    expect(router.currentRoute.value.name).toBe("other");
    expect(guard.abandonOpen.value).toBe(false);
    wrapper.unmount();
  });

  it("garde la question armée quand la navigation autorisée échoue : `allowed` retombe", async () => {
    const { router, state, guard, wrapper } = await setup({ dirty: true });
    // Une autre garde refuse la destination : la sortie autorisée n'aboutit pas.
    const remove = router.beforeEach((to) => (to.name === "other" ? false : true));
    await guard.leave({ name: "other" });
    expect(router.currentRoute.value.name).toBe("form");
    remove();
    // La saisie est toujours là : quitter redemande, et le rechargement aussi.
    expect(state.dirty).toBe(true);
    expect(unloadAsked()).toBe(true);
    await router.push("/other");
    expect(router.currentRoute.value.name).toBe("form");
    expect(guard.abandonOpen.value).toBe(true);
    wrapper.unmount();
  });
});
