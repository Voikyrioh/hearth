import { defineStore } from "pinia";
import { ref, watch } from "vue";
import { logUiError } from "@/errors/report";
import { t } from "@/i18n";
import {
  failureOf,
  getLinkBridge,
  type LinkState,
  type SecurityState,
  type Unsubscribe,
} from "@/link";
import { useLinkStore } from "./link";
import { useServersStore } from "./servers";
import { useToastsStore } from "./toasts";

/**
 * L'état de sécurité de chaque serveur (HRT-26) : l'alerte « attaque probable », le mode attaque, et ce
 * que l'agent dit de la session de ce poste. Alimenté par `link://security` (rejoué à l'abonnement,
 * ADR-0013 point 3) et relu à chaque retour du lien (BR-RESIL-010). Aucune règle ici : l'alerte et le
 * mode sont ceux de l'agent, le rôle est arbitré par lui. Un état dont `seq` n'est pas supérieur au
 * dernier connu est écarté (événement en retard, rejeu). Rien n'est persisté par la page.
 *
 * Une lecture ratée (lien coupé) ne vide jamais l'état connu : le bandeau garde ce qu'il sait, et
 * `lastKnown` dit depuis quand (une alerte ou un mode attaque ne s'estompe pas quand le lien tombe).
 */
export type SecurityStatus = "loading" | "ready" | "unsupported" | "error";

export interface ServerSecurity {
  status: SecurityStatus;
  /** Le dernier état connu ; `null` tant que rien n'a été reçu ni lu. */
  state: SecurityState | null;
  /** Heure (ms) de la dernière réception ou lecture réussie. */
  at: number | null;
}

export const useSecurityStore = defineStore("security", () => {
  const byServer = ref<Record<string, ServerSecurity>>({});
  /**
   * La confirmation d'activation ou de désactivation en cours (une seule à la fois, rendue par le
   * gabarit du serveur) : le bandeau d'alerte et la page Sécurité l'ouvrent au même endroit.
   */
  const dialog = ref<{ serverId: string; active: boolean } | null>(null);
  const reads = new Map<string, number>();
  const link = useLinkStore();
  const servers = useServersStore();
  const toasts = useToastsStore();
  let unsubscribe: Unsubscribe | null = null;
  let stopWatching: (() => void) | null = null;
  let started: Promise<void> | null = null;

  function of(serverId: string): ServerSecurity | undefined {
    return byServer.value[serverId];
  }

  function put(serverId: string, next: ServerSecurity) {
    byServer.value = { ...byServer.value, [serverId]: next };
  }

  /** Les annonces qui accompagnent un changement du mode : fin automatique, reprise après le redémarrage. */
  function announce(before: SecurityState | null, now: SecurityState) {
    if (!before) return;
    const was = before.attackMode.state;
    const is = now.attackMode.state;
    if (was !== "off" && is === "off" && now.attackMode.lastEnd === "auto") {
      toasts.push({ kind: "info", message: t("security.autoStopped") });
    }
    if (was === "suspended" && is === "active") {
      toasts.push({ kind: "info", message: t("security.resumed") });
    }
  }

  /** Un état reçu du flux, ou lu : écarté s'il n'est pas plus récent que le dernier connu. */
  function onState(state: SecurityState) {
    const known = byServer.value[state.serverId];
    if (known?.state && state.seq <= known.state.seq) return;
    // Le message du flux ne dit rien du poste : on garde ce que la dernière lecture en a dit.
    const merged: SecurityState =
      state.device === "unknown" && known?.state ? { ...state, device: known.state.device } : state;
    announce(known?.state ?? null, merged);
    put(state.serverId, { status: "ready", state: merged, at: Date.now() });
  }

  /** Lit (ou relit) l'état chez l'agent ; une réponse plus ancienne que la dernière demandée est écartée. */
  async function load(serverId: string): Promise<void> {
    const ticket = (reads.get(serverId) ?? 0) + 1;
    reads.set(serverId, ticket);
    const known = byServer.value[serverId];
    if (!known) put(serverId, { status: "loading", state: null, at: null });
    try {
      const read = await getLinkBridge().getSecurity(serverId);
      if (reads.get(serverId) !== ticket) return;
      if (read.kind === "unsupported") {
        put(serverId, { status: "unsupported", state: null, at: null });
        return;
      }
      // Un événement du flux arrivé pendant la lecture, plus récent, garde la main ; la lecture
      // n'apporte alors que le poste.
      const current = byServer.value[serverId]?.state;
      if (current && read.state.seq <= current.seq) {
        put(serverId, {
          status: "ready",
          state: { ...current, device: read.state.device, keyAtHand: read.state.keyAtHand },
          at: Date.now(),
        });
        return;
      }
      announce(current ?? null, read.state);
      put(serverId, { status: "ready", state: read.state, at: Date.now() });
    } catch (error) {
      if (reads.get(serverId) !== ticket) return;
      // Lien coupé ou serveur parti : le dernier état connu reste.
      const current = byServer.value[serverId];
      put(serverId, {
        status: "error",
        state: current?.state ?? null,
        at: current?.at ?? null,
      });
      if (!failureOf(error)) logUiError(error, "security:read");
    }
  }

  /**
   * Écoute `link://security` et relit l'état à chaque retour du lien (une seule fois). Appelé au
   * démarrage de l'application : la marque d'un serveur en alerte ou en mode attaque se lit dans la
   * barre des serveurs, quelle que soit la page.
   */
  function start(): Promise<void> {
    started ??= (async () => {
      try {
        unsubscribe = await getLinkBridge().onSecurity(onState);
      } catch (error) {
        logUiError(error, "security:subscribe");
      }
      let previous: Record<string, LinkState> = {};
      stopWatching = watch(
        () => Object.fromEntries(servers.servers.map((s) => [s.id, link.stateOf(s.id)])),
        (now) => {
          for (const [id, state] of Object.entries(now)) {
            if (state === "connected" && previous[id] !== "connected") void load(id);
          }
          previous = { ...now } as Record<string, LinkState>;
        },
        { immediate: true },
      );
    })();
    return started;
  }

  function ask(serverId: string, active: boolean) {
    dialog.value = { serverId, active };
  }

  function closeDialog() {
    dialog.value = null;
  }

  function forget(serverId: string) {
    const { [serverId]: _gone, ...rest } = byServer.value;
    byServer.value = rest;
    reads.delete(serverId);
  }

  /** Remise à zéro (tests). */
  function reset() {
    unsubscribe?.();
    unsubscribe = null;
    stopWatching?.();
    stopWatching = null;
    started = null;
    byServer.value = {};
    dialog.value = null;
    reads.clear();
  }

  return { byServer, dialog, of, load, start, ask, closeDialog, forget, reset };
});
