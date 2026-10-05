import { defineStore } from "pinia";
import { ref } from "vue";
import { reportUiError } from "@/errors/report";
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
/** Nouvelle tentative d'abonnement : espacement doublé de 1 s jusqu'à 30 s. */
export const RESUBSCRIBE_BASE_MS = 1000;
export const RESUBSCRIBE_MAX_MS = 30_000;

/**
 * État du lien de chaque serveur (BR-RESIL-020 : un état indépendant par serveur),
 * alimenté par le pont. Les issues d'opérations incertaines deviennent des notifications
 * discrètes qui nomment le serveur (BR-RESIL-010, 011) et restent consultables par `opId`.
 *
 * Abonnements : chaque désabonnement est gardé dès qu'il est obtenu ; `stop()` pendant une
 * attente n'en laisse aucun actif ; si l'abonnement échoue, `subscriptionFailed` passe à vrai
 * (les serveurs sans événement s'affichent « Hors ligne », jamais « Reconnexion… » pour
 * toujours) et une nouvelle tentative est faite, de plus en plus espacée.
 */
export const useLinkStore = defineStore("link", () => {
  const events = ref<Record<string, LinkStateEvent>>({});
  const operations = ref<Record<string, { event: OperationEvent; at: number }>>({});
  const subscriptionFailed = ref(false);
  const toasts = useToastsStore();
  const servers = useServersStore();
  let subscriptions: Unsubscribe[] = [];
  let generation = 0;
  let attempts = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let starting: Promise<void> | null = null;

  /** Écoute le pont (une seule fois). Rend la main quand la première tentative est finie. */
  function start(): Promise<void> {
    starting ??= attempt();
    return starting;
  }

  async function attempt(): Promise<void> {
    const mine = generation;
    const keep = (unsubscribe: Unsubscribe) => {
      if (mine === generation) subscriptions.push(unsubscribe);
      else unsubscribe();
    };
    try {
      const bridge = getLinkBridge();
      keep(await bridge.onLinkState(onState));
      if (mine !== generation) return;
      keep(await bridge.onOperation(onOperation));
      if (mine === generation) {
        subscriptionFailed.value = false;
        attempts = 0;
      }
    } catch (error) {
      if (mine !== generation) return;
      subscriptionFailed.value = true;
      reportUiError(error, "link:subscribe");
      releaseAll();
      const delay = Math.min(RESUBSCRIBE_BASE_MS * 2 ** attempts, RESUBSCRIBE_MAX_MS);
      attempts += 1;
      timer = setTimeout(() => {
        if (mine === generation) starting = attempt();
      }, delay);
    }
  }

  function releaseAll() {
    const old = subscriptions;
    subscriptions = [];
    for (const unsubscribe of old) unsubscribe();
  }

  // Un événement dont `seq` n'est pas supérieur au dernier connu est écarté (événement en
  // retard, rejeu d'un instantané) : l'horloge murale n'entre pas dans l'ordre.
  function onState(event: LinkStateEvent) {
    const known = events.value[event.serverId];
    if (known && event.seq <= known.seq) return;
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

  /** Arrête l'écoute : annule l'attente en cours, la nouvelle tentative et tout abonnement. */
  function stop() {
    generation += 1;
    clearTimeout(timer);
    starting = null;
    attempts = 0;
    subscriptionFailed.value = false;
    releaseAll();
  }

  function eventOf(serverId: string): LinkStateEvent | undefined {
    return events.value[serverId];
  }

  /** Sans événement : « Reconnexion… » le temps de s'abonner, « Hors ligne » si l'abonnement échoue. */
  function stateOf(serverId: string): LinkState {
    return events.value[serverId]?.state ?? (subscriptionFailed.value ? "offline" : "reconnecting");
  }

  async function retryNow(serverId: string) {
    if (subscriptionFailed.value) {
      // « Réessayer maintenant » relance aussi l'abonnement qui avait échoué.
      clearTimeout(timer);
      starting = attempt();
    }
    await getLinkBridge().retryNow(serverId);
  }

  /** Remise à zéro (tests). */
  function reset() {
    stop();
    events.value = {};
    operations.value = {};
  }

  return {
    events,
    operations,
    subscriptionFailed,
    start,
    stop,
    eventOf,
    stateOf,
    outcomeOf,
    retryNow,
    reset,
  };
});
