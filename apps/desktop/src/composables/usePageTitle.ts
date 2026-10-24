import {
  type InjectionKey,
  inject,
  onBeforeUnmount,
  provide,
  type Ref,
  ref,
  watchEffect,
} from "vue";

// Un titre d'en-tête que seule la page connaît (revue UX 2026-10-24, constat 17) : « Terraria » pour la fiche d'un jeu,
// « Installer Terraria » pour son installation, à chaque étape. Le gabarit du serveur appelle `providePageTitle` ; une
// page qui n'appelle pas `usePageTitle` garde le titre de sa route (`meta.title`). Tant que la page n'a pas lu son
// titre (`null`), l'en-tête garde celui de la route : jamais un en-tête vide.
const KEY: InjectionKey<Ref<string | null>> = Symbol("page-title");

/** Gabarit : le titre posé par la page, `null` par défaut. */
export function providePageTitle(): Ref<string | null> {
  const title = ref<string | null>(null);
  provide(KEY, title);
  return title;
}

/** Page : dit son titre (`null` : pas encore). Remis à `null` quand la page part. */
export function usePageTitle(title: () => string | null): void {
  const slot = inject(KEY, null);
  if (!slot) return;
  watchEffect(() => {
    slot.value = title();
  });
  onBeforeUnmount(() => {
    slot.value = null;
  });
}
