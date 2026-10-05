import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { getLinkBridge, type ServerInfo, type Unsubscribe } from "@/link";

/** Délai maximal pour s'abonner à la liste des serveurs : le garde du routeur n'attend pas sans fin. */
export const LOAD_TIMEOUT_MS = 5000;

/**
 * Serveurs enregistrés, alimentés par le pont de liaison. Le contrat du pont garantit le
 * rejeu de la liste courante avant que l'abonnement ne soit résolu : pas d'attente d'un
 * premier envoi. Si le pont ne répond pas dans `LOAD_TIMEOUT_MS`, `load()` échoue et
 * `loadFailed` passe à vrai (l'interface démarre sans serveur, l'accueil s'affiche).
 * `currentId` est le serveur affiché (posé par le gabarit de serveur d'après la route) :
 * `useNeedsLink` et les composants de la coquille y lisent « le serveur courant ».
 */
export const useServersStore = defineStore("servers", () => {
  const servers = ref<ServerInfo[]>([]);
  const loaded = ref(false);
  const loadFailed = ref(false);
  const currentId = ref<string | null>(null);
  let loading: Promise<void> | null = null;
  let unsubscribe: Unsubscribe | null = null;
  let generation = 0;

  const current = computed(() => servers.value.find((s) => s.id === currentId.value) ?? null);
  const first = computed(() => servers.value[0] ?? null);

  /** S'abonne une seule fois (nouvelle tentative possible après un échec). */
  function load(): Promise<void> {
    loading ??= subscribe().catch((error) => {
      loading = null;
      loadFailed.value = true;
      throw error;
    });
    return loading;
  }

  async function subscribe(): Promise<void> {
    const mine = generation;
    const subscription = getLinkBridge().onServersChanged((next) => {
      if (mine !== generation) return;
      servers.value = next;
      loaded.value = true;
      loadFailed.value = false;
    });
    let timer: ReturnType<typeof setTimeout> | undefined;
    const timeout = new Promise<never>((_, reject) => {
      timer = setTimeout(
        () => reject(new Error("liste des serveurs indisponible")),
        LOAD_TIMEOUT_MS,
      );
    });
    try {
      const unsub = await Promise.race([subscription, timeout]);
      if (mine === generation) unsubscribe = unsub;
      else unsub();
    } catch (error) {
      // Abonnement tardif : on le libère dès qu'il arrive plutôt que de le laisser fuir.
      void subscription.then((unsub) => unsub()).catch(() => {});
      throw error;
    } finally {
      clearTimeout(timer);
    }
  }

  function byId(id: string): ServerInfo | undefined {
    return servers.value.find((s) => s.id === id);
  }

  function setCurrent(id: string | null) {
    currentId.value = id;
  }

  /** Remise à zéro (tests). */
  function reset() {
    generation += 1;
    unsubscribe?.();
    unsubscribe = null;
    loading = null;
    servers.value = [];
    loaded.value = false;
    loadFailed.value = false;
    currentId.value = null;
  }

  return {
    servers,
    loaded,
    loadFailed,
    currentId,
    current,
    first,
    load,
    byId,
    setCurrent,
    reset,
  };
});
