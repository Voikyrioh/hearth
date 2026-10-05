import { defineStore } from "pinia";
import { ref } from "vue";
import { t } from "@/i18n";
import {
  getLinkBridge,
  type LinkState,
  type LinkStateEvent,
  type OperationEvent,
  type Unsubscribe,
} from "@/link";
import { useToastsStore } from "./toasts";

const OPERATION_TEXTS = {
  done: "operation.done",
  not_executed: "operation.notExecuted",
  unknown: "operation.unknown",
} as const;

/**
 * État du lien de chaque serveur (BR-RESIL-020 : un état indépendant par serveur),
 * alimenté par le pont. Les issues d'opérations incertaines deviennent des notifications
 * discrètes (BR-RESIL-010, 011).
 */
export const useLinkStore = defineStore("link", () => {
  const events = ref<Record<string, LinkStateEvent>>({});
  const toasts = useToastsStore();
  const subscriptions: Unsubscribe[] = [];

  /** Démarre l'écoute du pont (une seule fois). */
  function start() {
    if (subscriptions.length > 0) return;
    const bridge = getLinkBridge();
    subscriptions.push(
      bridge.onLinkState((event) => {
        events.value = { ...events.value, [event.serverId]: event };
      }),
      bridge.onOperation((event: OperationEvent) => {
        toasts.push({
          kind:
            event.outcome === "unknown" ? "warn" : event.outcome === "done" ? "success" : "info",
          message: t(OPERATION_TEXTS[event.outcome]),
        });
      }),
    );
  }

  function stop() {
    for (const unsubscribe of subscriptions.splice(0)) unsubscribe();
  }

  function eventOf(serverId: string): LinkStateEvent | undefined {
    return events.value[serverId];
  }

  /** Avant le premier événement d'un serveur, le lien est en cours d'établissement. */
  function stateOf(serverId: string): LinkState {
    return events.value[serverId]?.state ?? "reconnecting";
  }

  async function retryNow(serverId: string) {
    await getLinkBridge().retryNow(serverId);
  }

  /** Remise à zéro (tests). */
  function reset() {
    stop();
    events.value = {};
  }

  return { events, start, stop, eventOf, stateOf, retryNow, reset };
});
