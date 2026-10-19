/**
 * Le dernier geste de l'utilisateur, souris ou clavier, posé sur la racine (`data-input="pointer" | "keyboard"`).
 * Le navigateur décide seul si un focus posé par le code s'encadre (`:focus-visible`) ; après un clic sur un bouton
 * il encadre parfois quand même le titre où l'écran remet le focus. La feuille de style s'appuie sur cette marque
 * pour ne cadrer ces éléments qu'au clavier. FIX:01M5A5VJ8G6Q6FDJV9AX6ECF41
 */
export type InputModality = "pointer" | "keyboard";

/** Les touches qui ne disent pas « je navigue au clavier » (modificateurs seuls). */
const IGNORED_KEYS = new Set(["Shift", "Control", "Alt", "Meta", "AltGraph", "CapsLock"]);

/** Installe les écouteurs ; renvoie leur retrait. */
export function installInputModality(doc: Document): () => void {
  const root = doc.documentElement;
  const set = (value: InputModality) => {
    if (root.dataset.input !== value) root.dataset.input = value;
  };
  const onPointer = () => set("pointer");
  const onKey = (event: KeyboardEvent) => {
    if (!IGNORED_KEYS.has(event.key)) set("keyboard");
  };
  doc.addEventListener("pointerdown", onPointer, true);
  doc.addEventListener("keydown", onKey, true);
  return () => {
    doc.removeEventListener("pointerdown", onPointer, true);
    doc.removeEventListener("keydown", onKey, true);
  };
}
