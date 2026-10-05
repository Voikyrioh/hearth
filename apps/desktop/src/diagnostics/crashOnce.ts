let crashed = false;

/** Lève une erreur la première fois seulement : la page de diagnostic se rétablit au « Réessayer ». */
export function crashOnce(): void {
  if (crashed) return;
  crashed = true;
  throw new Error("diagnostic : rendu volontairement cassé");
}
