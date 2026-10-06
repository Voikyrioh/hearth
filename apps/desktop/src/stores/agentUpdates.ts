import { defineStore } from "pinia";
import { computed, ref, watch } from "vue";
import { resultMessage, resultTone } from "@/agentUpdate/messages";
import { logUiError } from "@/errors/report";
import { t } from "@/i18n";
import {
  type AgentUpdateEvent,
  type AgentUpdateView,
  failureOf,
  getLinkBridge,
  type LinkState,
  type Unsubscribe,
  type UpdateProgress,
  type UpdateResult,
} from "@/link";
import { useLinkStore } from "./link";
import { useServersStore } from "./servers";
import { useToastsStore } from "./toasts";

/** Un résultat à annoncer dans la carte du serveur : son ton, son texte, et de quoi l'écarter. */
export interface ResultNotice {
  key: string;
  tone: "ok" | "warn" | "crit";
  message: string;
}

interface Entry {
  /** La dernière lecture de l'état chez l'agent (jamais inventée : celle de `getAgentUpdate`). */
  view: AgentUpdateView | null;
  status: "loading" | "ready" | "error";
  /** La dernière progression reçue du flux (la fin comprise, jusqu'à ce que la lecture la reprenne). */
  live: UpdateProgress | null;
  /** Le résultat déjà lu et fermé par l'utilisateur (clé de `notice`). */
  dismissed: string | null;
}

/** Une clé qui désigne un résultat, d'où qu'il vienne (le flux en direct ou la lecture). */
function resultKey(result: {
  outcome: string;
  reason: string | null;
  version: string | null;
}): string {
  return [result.outcome, result.reason ?? "", result.version ?? ""].join("|");
}

/**
 * La mise à jour de l'agent de chaque serveur (HRT-17) : l'état lu chez l'agent, la progression reçue
 * du flux, et ce qu'il faut en dire. Rien n'est calculé ici qui soit une règle : la version
 * « disponible » est décidée par la coquille, le rôle par l'agent, la coupure attendue par la liaison.
 * Rien n'est persisté par la page (ADR-0010).
 *
 * L'état se relit à chaque retour du lien (BR-UPDATE-017) : une lecture plus ancienne qu'un événement
 * arrivé depuis ne l'écrase pas. Une coupure pendant la mise à jour n'est PAS une erreur : le lien dit
 * « Reconnexion… » (BR-UPDATE-014), la carte garde ses étapes.
 */
export const useAgentUpdatesStore = defineStore("agentUpdates", () => {
  const byServer = ref<Record<string, Entry>>({});
  const reads = new Map<string, number>();
  /** Événements reçus par serveur : une lecture commencée avant le dernier ne l'écrase pas. */
  const eventCounts = new Map<string, number>();
  const link = useLinkStore();
  const servers = useServersStore();
  const toasts = useToastsStore();
  let unsubscribe: Unsubscribe | null = null;
  let stopWatching: (() => void) | null = null;
  let started: Promise<void> | null = null;

  function entry(serverId: string): Entry {
    return (
      byServer.value[serverId] ?? { view: null, status: "loading", live: null, dismissed: null }
    );
  }

  function put(serverId: string, patch: Partial<Entry>) {
    byServer.value = { ...byServer.value, [serverId]: { ...entry(serverId), ...patch } };
  }

  /** La mise à jour en cours (étape, pourcentage), d'après le flux, sinon d'après la dernière lecture. */
  function progressOf(serverId: string): UpdateProgress | null {
    const known = entry(serverId);
    if (known.live) return known.live.step === "done" ? null : known.live;
    return known.view?.progress ?? null;
  }

  /** Une mise à jour travaille (chez l'agent, ou annoncée par le flux). */
  function runningOf(serverId: string): boolean {
    return progressOf(serverId) !== null || entry(serverId).view?.inProgress === true;
  }

  function noticeOf(serverId: string): ResultNotice | null {
    const known = entry(serverId);
    const fromLast = (result: UpdateResult): ResultNotice => ({
      key: resultKey(result),
      tone: resultTone(result),
      message: resultMessage(result),
    });
    let notice: ResultNotice | null = null;
    const live = known.live;
    if (live?.step === "done" && live.outcome) {
      const result = { outcome: live.outcome, reason: live.reason, version: live.version || null };
      notice = { key: resultKey(result), tone: resultTone(result), message: resultMessage(result) };
    } else if (known.view?.last?.recent && !runningOf(serverId)) {
      notice = fromLast(known.view.last);
    }
    return notice && notice.key !== known.dismissed ? notice : null;
  }

  /** Lit (ou relit) l'état chez l'agent ; une réponse plus ancienne que la dernière demandée est écartée. */
  async function refresh(serverId: string): Promise<void> {
    const ticket = (reads.get(serverId) ?? 0) + 1;
    reads.set(serverId, ticket);
    const eventsBefore = eventCounts.get(serverId) ?? 0;
    if (!byServer.value[serverId]) put(serverId, { status: "loading" });
    try {
      const view = await getLinkBridge().getAgentUpdate(serverId);
      if (reads.get(serverId) !== ticket) return;
      const newerEvent = (eventCounts.get(serverId) ?? 0) !== eventsBefore;
      const patch: Partial<Entry> = { view, status: "ready" };
      // La lecture fait foi tant qu'aucun événement n'est arrivé depuis : le flux, plus récent,
      // garde la main sinon. Rien en cours chez l'agent : le direct est périmé (sauf la fin
      // annoncée, que la lecture reprend à son compte).
      if (!newerEvent) {
        const live = entry(serverId).live;
        if (live && (live.step !== "done" || view.last)) patch.live = null;
      }
      put(serverId, patch);
    } catch (error) {
      if (reads.get(serverId) !== ticket) return;
      // Lien coupé ou serveur parti : la dernière lecture reste (la carte le dit, sans alarme).
      put(serverId, { status: "error" });
      if (!failureOf(error)) logUiError(error, "agent-update:read");
    }
  }

  function onProgress(event: AgentUpdateEvent) {
    const { serverId, progress } = event;
    eventCounts.set(serverId, (eventCounts.get(serverId) ?? 0) + 1);
    // Une nouvelle mise à jour (première étape) : le résultat précédent, même identique, se
    // montrera de nouveau à sa fin.
    const fresh = progress.step === "download" && entry(serverId).live?.step !== "download";
    put(serverId, { live: progress, ...(fresh ? { dismissed: null } : {}) });
    if (progress.step === "done") {
      announce(serverId, progress);
      // Le dernier résultat et la version de l'agent se relisent (BR-UPDATE-017).
      void refresh(serverId);
    }
  }

  /** Une notification discrète à la fin (l'utilisateur est peut-être ailleurs que dans les réglages). */
  function announce(serverId: string, progress: UpdateProgress) {
    if (!progress.outcome) return;
    const server = servers.byId(serverId);
    const result = {
      outcome: progress.outcome,
      reason: progress.reason,
      version: progress.version || null,
    };
    const tone = resultTone(result);
    toasts.push({
      kind: tone === "ok" ? "success" : tone === "warn" ? "warn" : "error",
      message: t("agentUpdate.toastDone", {
        server: server?.name ?? serverId,
        message: resultMessage(result),
      }),
    });
  }

  /** La demande a été acceptée : les étapes s'affichent tout de suite (le flux les confirmera). */
  function begin(serverId: string, version: string) {
    put(serverId, {
      live: { version, step: "download", percent: null, outcome: null, reason: null },
      dismissed: null,
    });
  }

  function dismiss(serverId: string) {
    const notice = noticeOf(serverId);
    if (notice) put(serverId, { dismissed: notice.key });
  }

  /**
   * Écoute le flux de progression et relit l'état à chaque retour du lien (une seule fois). Appelé au
   * démarrage de l'application : la version « disponible » de chaque serveur connecté s'y lit aussi
   * (mention de la liste des serveurs, BR-UPDATE-023).
   */
  function start(): Promise<void> {
    started ??= (async () => {
      try {
        unsubscribe = await getLinkBridge().onAgentUpdate(onProgress);
      } catch (error) {
        logUiError(error, "agent-update:subscribe");
      }
      let previous: Record<string, LinkState> = {};
      stopWatching = watch(
        () => Object.fromEntries(servers.servers.map((s) => [s.id, link.stateOf(s.id)])),
        (now) => {
          for (const [id, state] of Object.entries(now)) {
            if (state === "connected" && previous[id] !== "connected") void refresh(id);
          }
          previous = { ...now } as Record<string, LinkState>;
        },
        { immediate: true },
      );
    })();
    return started;
  }

  function forget(serverId: string) {
    const { [serverId]: _gone, ...rest } = byServer.value;
    byServer.value = rest;
    reads.delete(serverId);
    eventCounts.delete(serverId);
  }

  /** Remise à zéro (tests). */
  function reset() {
    unsubscribe?.();
    unsubscribe = null;
    stopWatching?.();
    stopWatching = null;
    started = null;
    byServer.value = {};
    reads.clear();
    eventCounts.clear();
  }

  /** Les serveurs qui ont une mise à jour de l'agent disponible (BR-UPDATE-023). */
  const withUpdate = computed(() =>
    Object.entries(byServer.value)
      .filter(([, known]) => known.view?.available && !known.view.managed)
      .map(([id]) => id),
  );

  return {
    byServer,
    entry,
    progressOf,
    runningOf,
    noticeOf,
    refresh,
    begin,
    dismiss,
    start,
    forget,
    reset,
    withUpdate,
  };
});
