import { flushPromises } from "@vue/test-utils";

/** Le mot de passe du compte simulé : celui que la confirmation des actes compare. */
export const OWN = "Correct-Horse-9";

/** Saisit une valeur dans un champ, comme au clavier. */
export function typeInto(el: Element | null, value: string) {
  const input = el as HTMLInputElement;
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

/** Le bouton d'une fenêtre ouverte, par son texte. */
export function dialogButton(label: string): HTMLButtonElement | undefined {
  return [...document.querySelectorAll<HTMLButtonElement>("dialog button")].find(
    (button) => button.textContent?.trim() === label,
  );
}

/** Le champ « Ton mot de passe » de la confirmation d'un acte, s'il est affiché. */
export function reauthField(): HTMLInputElement | null {
  return document.querySelector<HTMLInputElement>("dialog [data-reauth-field] input");
}

/**
 * Dans la fenêtre ouverte d'un acte d'administration : attend que l'état de la confirmation soit lu
 * (l'interface ne devine pas l'élévation), saisit le mot de passe s'il est demandé (`null` : n'en saisit
 * pas), puis clique sur le bouton de confirmation.
 */
export async function confirmDialog(label: string, password: string | null = OWN) {
  await flushPromises();
  const field = reauthField();
  if (field && password !== null) {
    typeInto(field, password);
    await flushPromises();
  }
  dialogButton(label)?.click();
  await flushPromises();
}
