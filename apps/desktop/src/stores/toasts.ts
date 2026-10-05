import { defineStore } from "pinia";
import { computed, ref } from "vue";

export type ToastKind = "info" | "success" | "warn" | "error";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
  /** Nombre de fois que cette même notification a été émise (1 = une seule). */
  count: number;
  /** Clé d'une notification qui se met à jour sur place (compteur d'échecs, par exemple). */
  key?: string;
}

/** Notifications visibles en même temps (les autres attendent leur tour). */
export const MAX_VISIBLE_TOASTS = 3;
/** Garde-fou : la file ne grossit jamais sans limite (BR-RESIL-017). */
const MAX_QUEUED_TOASTS = 50;
/** Durée d'affichage avant disparition automatique. */
export const TOAST_LIFETIME_MS = 6000;

/**
 * Notifications discrètes (BR-RESIL-011, 018) : jamais bloquantes, fermables, et une
 * même notification répétée devient un compteur au lieu de se multiplier.
 */
export const useToastsStore = defineStore("toasts", () => {
  const items = ref<Toast[]>([]);
  const timers = new Map<number, ReturnType<typeof setTimeout>>();
  let nextId = 1;

  const visible = computed(() => items.value.slice(-MAX_VISIBLE_TOASTS));

  function arm(id: number) {
    clearTimeout(timers.get(id));
    timers.set(
      id,
      setTimeout(() => dismiss(id), TOAST_LIFETIME_MS),
    );
  }

  function push(toast: { kind: ToastKind; message: string; key?: string }): number {
    // Une notification à clé se met à jour sur place : « Reconnexion échouée 4 fois » devient « 5 fois »
    // au lieu d'ajouter une ligne (BR-RESIL-018).
    const keyed = toast.key ? items.value.find((item) => item.key === toast.key) : undefined;
    if (keyed) {
      keyed.kind = toast.kind;
      keyed.message = toast.message;
      arm(keyed.id);
      return keyed.id;
    }
    const existing = items.value.find(
      (item) => item.kind === toast.kind && item.message === toast.message,
    );
    if (existing) {
      existing.count += 1;
      arm(existing.id);
      return existing.id;
    }
    const id = nextId++;
    items.value.push({ id, kind: toast.kind, message: toast.message, count: 1, key: toast.key });
    arm(id);
    while (items.value.length > MAX_QUEUED_TOASTS) {
      const dropped = items.value.shift();
      if (dropped) dismiss(dropped.id);
    }
    return id;
  }

  function dismiss(id: number) {
    clearTimeout(timers.get(id));
    timers.delete(id);
    items.value = items.value.filter((item) => item.id !== id);
  }

  /** Retire la notification à cette clé, si elle existe (le lien est revenu). */
  function dismissKey(key: string) {
    const found = items.value.find((item) => item.key === key);
    if (found) dismiss(found.id);
  }

  function clear() {
    for (const id of [...timers.keys()]) dismiss(id);
    items.value = [];
  }

  return { items, visible, push, dismiss, dismissKey, clear };
});
