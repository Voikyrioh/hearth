import {
  type InjectionKey,
  inject,
  onBeforeUnmount,
  provide,
  type Ref,
  ref,
  watchEffect,
} from "vue";

/**
 * Un emplacement que le gabarit du serveur offre à la page affichée, et que la page remplit (`usePageData`,
 * `usePageTitle`). Le gabarit appelle `provide` ; une page appelle `use` avec un lecteur, relu à chaque changement ; quand
 * la page part, l'emplacement revient à sa valeur de repos.
 */
export function pageSlot<T>(name: string, rest: T) {
  const key: InjectionKey<Ref<T>> = Symbol(name);
  return {
    provide(): Ref<T> {
      const slot = ref(rest) as Ref<T>;
      provide(key, slot);
      return slot;
    },
    use(read: () => T): void {
      const slot = inject(key, null);
      if (!slot) return;
      watchEffect(() => {
        slot.value = read();
      });
      onBeforeUnmount(() => {
        slot.value = rest;
      });
    },
  };
}
