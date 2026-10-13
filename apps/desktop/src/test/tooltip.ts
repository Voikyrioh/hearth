import { DOMWrapper } from "@vue/test-utils";

// Les bulles d'explication sont rendues dans `body` (Teleport, HRT-134) : elles ne sont plus dans le
// wrapper du composant. Ces aides les lisent là où elles sont. Le nettoyage entre deux tests est fait par
// `setup.ts` (démontage automatique).
export function hasTooltip(): boolean {
  return document.body.querySelector('[role="tooltip"]') !== null;
}

/** La première bulle de la page ; échoue s'il n'y en a pas (comme `wrapper.get`). */
export function tooltip(): DOMWrapper<HTMLElement> {
  const element = document.body.querySelector<HTMLElement>('[role="tooltip"]');
  if (!element) throw new Error('Aucune bulle [role="tooltip"] dans la page');
  return new DOMWrapper(element);
}

/** La bulle qui décrit ce contrôle (par son `aria-describedby`) : celle de CE bouton, pas la première de la page. */
export function tooltipFor(control: {
  attributes(name: string): string | undefined;
}): DOMWrapper<HTMLElement> {
  const ids = (control.attributes("aria-describedby") ?? "").split(" ");
  for (const id of ids) {
    const element = document.getElementById(id);
    if (element?.getAttribute("role") === "tooltip") return new DOMWrapper(element);
  }
  throw new Error("Ce contrôle n'est décrit par aucune bulle");
}
