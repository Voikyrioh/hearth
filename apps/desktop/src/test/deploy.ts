import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, beforeEach, vi } from "vitest";
import App from "@/App.vue";
import { CHECK_DELAY_MS } from "@/composables/useDeployForm";
import type { SimulatedLinkBridge } from "@/link";
import { GIB } from "@/resources";
import { useDeployFormStore } from "@/stores/deployForm";
import { useServiceOpsStore } from "@/stores/serviceOps";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";
import { useToastsStore } from "@/stores/toasts";
import { OWN, typeInto } from "@/test/confirm";
import { mountContext } from "@/test/mount";

/** Horloge simulée et remise à zéro des magasins entre deux tests de déploiement (HRT-69). */
export function useDeployTestClock() {
  beforeEach(() =>
    vi.useFakeTimers({
      toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
    }),
  );
  afterEach(() => {
    // Un test qui échoue n'a pas démonté son application : on le fait pour que le suivant reparte à neuf.
    for (const wrapper of mounted.splice(0)) {
      try {
        wrapper.unmount();
      } catch {
        // Déjà démontée.
      }
    }
    vi.useRealTimers();
    document.body.innerHTML = "";
    useServicesStore().reset();
    useServiceOpsStore().reset();
    useServiceRightsStore().reset();
    useDeployFormStore().wipeAll();
    useToastsStore().items.splice(0);
  });
}

const mounted: VueWrapper[] = [];

/** L'application complète sur la route du formulaire, un service « base » déjà là. */
export async function boot(
  setup?: (bridge: SimulatedLinkBridge) => void,
  path = "/servers/forge/services/deployer",
  server = "forge",
) {
  const ctx = await mountContext();
  ctx.bridge.services.seed(server, [
    { key: "h_db", name: "base", origin: "hearth", state: "running" },
  ]);
  // Les pages se testent sur une petite machine (4 cœurs, 16 Go) ; la machine d'exemple du simulateur est celle du
  // tableau de bord (16 cœurs, 64 Go).
  ctx.bridge.deploys.setCapacity(server, { cpus: 4, memoryBytes: 16 * GIB });
  setup?.(ctx.bridge);
  await ctx.router.push(path);
  await ctx.router.isReady();
  const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
  mounted.push(wrapper as unknown as VueWrapper);
  await settle();
  return { ...ctx, wrapper };
}

/** Laisse passer le délai de la consultation du serveur et les promesses qui suivent. */
export async function settle() {
  await flushPromises();
  await vi.advanceTimersByTimeAsync(CHECK_DELAY_MS + 50);
  await flushPromises();
}

export const field = (wrapper: VueWrapper, name: string) =>
  wrapper.get<HTMLInputElement>(`[data-field="${name}"] input`);

export async function type(wrapper: VueWrapper, name: string, value: string) {
  typeInto(field(wrapper, name).element, value);
  await settle();
}

export async function blur(wrapper: VueWrapper, name: string) {
  await field(wrapper, name).trigger("blur");
  await flushPromises();
}

export async function fillMinimum(wrapper: VueWrapper) {
  await type(wrapper, "name", "web");
  await type(wrapper, "image", "nginx");
}

export const submit = (wrapper: VueWrapper) => wrapper.get("[data-deploy-submit]");
export const reason = (wrapper: VueWrapper) => wrapper.find("[data-deploy-reason]").text();
export const toasts = () => useToastsStore().items.map((toast) => toast.message);
/** Le texte de TOUTES les fenêtres ouvertes (une confirmation peut s'ouvrir par-dessus la progression). */
export const dialogText = () =>
  [...document.querySelectorAll("dialog[open]")]
    .map((dialog) => dialog.textContent ?? "")
    .join(" ");

export async function openSummary(wrapper: VueWrapper) {
  await submit(wrapper).trigger("click");
  await settle();
}

/** Le bouton de la fenêtre la plus récente (une confirmation s'ouvre par-dessus la progression), par son texte. */
export function topButton(label: string): HTMLButtonElement | undefined {
  const dialogs = [...document.querySelectorAll("dialog[open]")];
  const top = dialogs[dialogs.length - 1];
  return [...(top?.querySelectorAll("button") ?? [])].find(
    (button) => button.textContent?.trim() === label,
  );
}

/** Comme `confirmDialog`, mais dans la fenêtre la plus récente : saisit le mot de passe puis clique. */
export async function confirmTop(label: string, password: string | null = OWN) {
  await flushPromises();
  const dialogs = [...document.querySelectorAll("dialog[open]")];
  const top = dialogs[dialogs.length - 1];
  const field = top?.querySelector<HTMLInputElement>("[data-reauth-field] input");
  if (field && password !== null) {
    typeInto(field, password);
    await flushPromises();
  }
  topButton(label)?.click();
  await flushPromises();
}
