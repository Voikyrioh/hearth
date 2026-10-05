import { defineStore } from "pinia";
import { ref } from "vue";
import { logUiError } from "@/errors/report";
import { t } from "@/i18n";
import {
  type FingerprintChange,
  getLinkBridge,
  type LinkNotice,
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
/** À partir de ce nombre d'échecs de reconnexion consécutifs, une notification discrète les compte. */
export const RECONNECT_FAILURE_NOTICE_FROM = 3;
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
 * (les serveurs s'affichent « Hors ligne », jamais « Connecté » ni « Reconnexion… » pour
 * toujours : sans écouteur actif, rien n'est à jour) avec un message clair, et une nouvelle
 * tentative est faite, de plus en plus espacée ; deux demandes en même temps (double clic sur
 * « Réessayer ») ne font qu'une tentative.
 *
 * Alertes d'empreinte (BR-CONN-003) : une par serveur, bloquantes tant qu'elles ne sont pas
 * tranchées ; « Ne pas se connecter » les masque (la liaison reste suspendue), un nouvel avis ou
 * « Voir l'alerte » les rouvre.
 */
export const useLinkStore = defineStore("link", () => {
  const events = ref<Record<string, LinkStateEvent>>({});
  const operations = ref<Record<string, { event: OperationEvent; at: number }>>({});
  const subscriptionFailed = ref(false);
  /** Vrai quand TOUS les abonnements sont actifs : seulement alors les états reçus sont à jour. */
  const listening = ref(false);
  const alerts = ref<Record<string, FingerprintChange>>({});
  /** Alertes masquées : serveur vers l'empreinte reçue qui a été refusée. */
  const dismissed = ref<Record<string, string>>({});
  const toasts = useToastsStore();
  const servers = useServersStore();
  let subscriptions: Unsubscribe[] = [];
  let generation = 0;
  let attempts = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let starting: Promise<void> | null = null;
  let attempting = false;

  /** Écoute le pont (une seule fois). Rend la main quand la première tentative est finie. */
  function start(): Promise<void> {
    starting ??= attempt();
    return starting;
  }

  async function attempt(): Promise<void> {
    const mine = generation;
    attempting = true;
    const keep = (unsubscribe: Unsubscribe) => {
      if (mine === generation) subscriptions.push(unsubscribe);
      else unsubscribe();
    };
    try {
      const bridge = getLinkBridge();
      keep(await bridge.onLinkState(onState));
      if (mine !== generation) return;
      keep(await bridge.onOperation(onOperation));
      if (mine !== generation) return;
      keep(await bridge.onFingerprintChanged(onFingerprint));
      if (mine !== generation) return;
      keep(await bridge.onNotice(onNotice));
      if (mine === generation) {
        subscriptionFailed.value = false;
        listening.value = true;
        attempts = 0;
      }
    } catch (error) {
      if (mine !== generation) return;
      subscriptionFailed.value = true;
      listening.value = false;
      // Un message clair pour l'utilisateur ; le détail technique va au journal seulement.
      toasts.push({ kind: "warn", message: t("bridge.subscribeFailed") });
      logUiError(error, "link:subscribe");
      releaseAll();
      const delay = Math.min(RESUBSCRIBE_BASE_MS * 2 ** attempts, RESUBSCRIBE_MAX_MS);
      attempts += 1;
      timer = setTimeout(() => {
        if (mine === generation) starting = attempt();
      }, delay);
    } finally {
      if (mine === generation) attempting = false;
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
    reportReconnectFailures(event);
    // L'alerte d'empreinte ne survit pas à la levée du blocage (acceptée, ou serveur revenu).
    if (event.blocked !== "fingerprint_changed" && event.serverId in alerts.value) {
      const { [event.serverId]: _gone, ...rest } = alerts.value;
      alerts.value = rest;
    }
  }

  // Coupures répétées (BR-RESIL-018) : une seule notification par serveur, dont le compteur monte,
  // jamais une ligne par tentative ; elle disparaît quand le lien revient.
  function reportReconnectFailures(event: LinkStateEvent) {
    const key = `reconnect:${event.serverId}`;
    const retrying = event.state === "reconnecting" || event.state === "offline";
    if (retrying && event.failedAttempts >= RECONNECT_FAILURE_NOTICE_FROM) {
      const name = servers.byId(event.serverId)?.name ?? event.serverId;
      toasts.push({
        key,
        kind: "warn",
        message: t("operation.withServer", {
          server: name,
          message: t("link.reconnectFailed", { n: event.failedAttempts }),
        }),
      });
    } else if (!retrying || event.failedAttempts === 0) {
      toasts.dismissKey(key);
    }
  }

  function onFingerprint(change: FingerprintChange) {
    alerts.value = { ...alerts.value, [change.serverId]: change };
    // Une alerte déjà refusée ne rouvre pas à un rejeu ; une empreinte différente, si.
    if (dismissed.value[change.serverId] !== change.presentedHex) {
      const { [change.serverId]: _old, ...rest } = dismissed.value;
      dismissed.value = rest;
    }
  }

  // Avis déjà montrés (signal en direct et lecture d'état peuvent porter le même) : bornés.
  const seenNotices = new Set<number>();

  function onNotice(notice: LinkNotice) {
    if (notice.kind !== "operations_lost") return;
    if (notice.id > 0) {
      if (seenNotices.has(notice.id)) return;
      seenNotices.add(notice.id);
      if (seenNotices.size > 200) seenNotices.delete(seenNotices.values().next().value as number);
    }
    const name = notice.serverId ? (servers.byId(notice.serverId)?.name ?? notice.serverId) : "";
    toasts.push({ kind: "warn", message: t("bridge.operationsLost", { server: name }) });
  }

  /** L'alerte d'empreinte à montrer pour ce serveur, ou `undefined` (aucune, ou déjà refusée). */
  function pendingAlert(serverId: string): FingerprintChange | undefined {
    const alert = alerts.value[serverId];
    return alert && dismissed.value[serverId] !== alert.presentedHex ? alert : undefined;
  }

  /** « Ne pas se connecter » : l'alerte se ferme, le lien reste suspendu. */
  function dismissAlert(serverId: string) {
    const alert = alerts.value[serverId];
    if (alert) dismissed.value = { ...dismissed.value, [serverId]: alert.presentedHex };
  }

  /** « Voir l'alerte » : la rouvre. */
  function reopenAlert(serverId: string) {
    const { [serverId]: _gone, ...rest } = dismissed.value;
    dismissed.value = rest;
  }

  /** L'utilisateur accepte la nouvelle empreinte : elle remplace l'ancienne et le lien repart. */
  async function acceptAlert(serverId: string) {
    const alert = alerts.value[serverId];
    if (!alert) return;
    await getLinkBridge().acceptFingerprint(serverId, alert.presentedHex);
    const { [serverId]: _gone, ...rest } = alerts.value;
    alerts.value = rest;
  }

  function onOperation(event: OperationEvent) {
    // Le même `opId` ne se montre qu'une fois (signal en direct et lecture d'état).
    if (event.opId in operations.value) return;
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
    attempting = false;
    subscriptionFailed.value = false;
    listening.value = false;
    releaseAll();
  }

  function eventOf(serverId: string): LinkStateEvent | undefined {
    return events.value[serverId];
  }

  /**
   * Sans écouteur actif, rien n'est à jour : « Reconnexion… » le temps de s'abonner, « Hors
   * ligne » si l'abonnement a échoué (même si un événement plus ancien disait « Connecté »).
   */
  function stateOf(serverId: string): LinkState {
    if (!listening.value) return subscriptionFailed.value ? "offline" : "reconnecting";
    return events.value[serverId]?.state ?? "reconnecting";
  }

  async function retryNow(serverId: string) {
    if (subscriptionFailed.value && !attempting) {
      // « Réessayer maintenant » relance aussi l'abonnement qui avait échoué (une seule fois,
      // même sur un double clic).
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
    alerts.value = {};
    dismissed.value = {};
  }

  return {
    events,
    operations,
    subscriptionFailed,
    listening,
    alerts,
    pendingAlert,
    dismissAlert,
    reopenAlert,
    acceptAlert,
    start,
    stop,
    eventOf,
    stateOf,
    outcomeOf,
    retryNow,
    reset,
  };
});
