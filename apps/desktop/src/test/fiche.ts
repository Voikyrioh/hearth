import { flushPromises, type VueWrapper } from "@vue/test-utils";
import { afterEach } from "vitest";
import type { SimulatedLinkBridge } from "@/link";
import { useFicheInstallStore } from "@/stores/ficheInstall";
import { typeInto } from "@/test/confirm";
import { boot, settle, useDeployTestClock } from "@/test/deploy";

export { confirmTop, dialogText, settle, toasts, topButton } from "@/test/deploy";

/** Horloge simulée et remise à zéro des magasins entre deux tests du Catalogue (HRT-102). */
export function useFicheTestClock() {
  useDeployTestClock();
  afterEach(() => useFicheInstallStore().wipeAll());
}

/** L'application complète sur une route du Catalogue (serveur « forge » : administrateur avec le droit de gérer les services). */
export function bootCatalogue(
  setup?: (bridge: SimulatedLinkBridge) => void,
  path = "/servers/forge/catalogue",
  server = "forge",
) {
  return boot(setup, path, server);
}

/** La page d'installation d'une fiche, sur le serveur « forge ». */
export const installPath = (fiche: string, query = "") =>
  `/servers/forge/catalogue/${fiche}/installer${query}`;

/** Le champ de saisie d'une question de la fiche, par son identifiant. */
export function questionInput(wrapper: VueWrapper, id: string) {
  return wrapper.get<HTMLInputElement>(`[data-question="${id}"] input`);
}

export async function answer(wrapper: VueWrapper, id: string, value: string) {
  typeInto(questionInput(wrapper, id).element, value);
  await settle();
}

export async function setName(wrapper: VueWrapper, value: string) {
  typeInto(wrapper.get<HTMLInputElement>('[data-field="name"] input').element, value);
  await settle();
}

export const next = (wrapper: VueWrapper) => wrapper.get("[data-install-next]");
export const reasonText = (wrapper: VueWrapper) => wrapper.find("[data-install-reason]").text();

/** Clique sur « Continuer » et laisse passer les lectures du serveur. */
export async function goNext(wrapper: VueWrapper) {
  await next(wrapper).trigger("click");
  await settle();
  await flushPromises();
}
