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
import { useServersStore } from "./servers";
import { useToastsStore } from "./toasts";

const OPERATION_TEXTS = {
  done: "operation.done",
  not_executed: "operation.notExecuted",
  unknown: "operation.unknown",
} as const;

/** Issues d'opération gardées (nombre et âge bornés : session longue, BR-RESIL-017). */
export const MAX_OPERATIONS = 100;
export const OPERATION_MAX_AGE_MS = 10 * 60_000;

/**
 * État du lien de chaque serveur (BR-RESIL-020 : un état indépendant par serveur),
 * alimenté par le pont. Les issues d'opérations incertaines deviennent des notifications
 * discrètes qui nomment le serveur (BR-RESIL-010, 011) et restent consultables par `opId`.
 */
export const useLinkStore = defineStore("link", () => {
  const events = ref<Record<string, LinkStateEvent>>({});
  const operations = ref<Record<string, { event: OperationEvent; at: number }>>({});
  const toasts = useToastsStore();
  const servers = useServersStore();
  const subscriptions: Unsubscribe[] = [];
  let starting: Promise<void> | null = null;

  /** Écoute le pont (une seule fois). Rend la main quand les états courants sont arrivés. */
  function start(): Promise<void> {
    starting ??= (async () => {
      const bridge = getLinkBridge();
      subscriptions.push(await bridge.onLinkState(onState), await bridge.onOperation(onOperation));
    })().catch((error) => {
      starting = null;
      throw error;
    });
    return starting;
  }

  // Un événement plus ancien (`since`) n'écrase jamais un plus récent : avec le vrai pont,
  // un instantané en retard peut arriver après un changement d'état.
  function onState(event: LinkStateEvent) {
    const known = events.value[event.serverId];
    if (known && event.since < known.since) return;
    events.value = { ...events.value, [event.serverId]: event };
  }

  function onOperation(event: OperationEvent) {
    record(event, Date.now());
    const name = servers.byId(event.serverId)?.name ?? event.serverId;
    toasts.push({
      kind: event.outcome === "unknown" ? "warn" : event.outcome === "done" ? "success" : "info",
      message: t("operation.withServer", {
        server: name,
        message: t(OPERATION_TEXTS[event.outcome]),
      }),
    });
  }

  function record(event: OperationEvent, now: number) {
    const kept = Object.entries(operations.value)
      .filter(([, entry]) => now - entry.at < OPERATION_MAX_AGE_MS)
      .sort(([, a], [, b]) => a.at - b.at);
    const next = Object.fromEntries(kept.slice(-(MAX_OPERATIONS - 1)));
    next[event.opId] = { event, at: now };
    operations.value = next;
  }

  /** Issue connue de l'opération `opId` (celle de SON action), ou `undefined`. */
  function outcomeOf(opId: string): OperationEvent["outcome"] | undefined {
    return operations.value[opId]?.event.outcome;
  }

  function stop() {
    starting = null;
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
    operations.value = {};
  }

  return { events, operations, start, stop, eventOf, stateOf, outcomeOf, retryNow, reset };
});
