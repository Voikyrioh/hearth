import { computed, ref } from "vue";
import { useLifecycleActions } from "@/composables/useLifecycleActions";
import { useOperationFollow } from "@/composables/useOperationFollow";
import { type ActReport, useReauth } from "@/composables/useReauth";
import { logUiError } from "@/errors/report";
import { t } from "@/i18n";
import {
  failureOf,
  type GameView,
  getLinkBridge,
  type ServiceInfo,
  type ServiceOperation,
} from "@/link";
import { alreadyText, type Gesture, refusalText } from "@/services/lifecycle";
import { useGameSettingsPendingStore } from "@/stores/gameSettingsPending";
import { useToastsStore } from "@/stores/toasts";

/** Le geste en attente de confirmation : le service visé (par sa clé stable) et ce que l'on veut en faire. */
export interface PendingGesture {
  /** Le service tel qu'il était à l'ouverture : de quoi garder son nom si la liste le perd (BR-LIFE-022). */
  service: ServiceInfo;
  gesture: Gesture;
  /**
   * Ce que l'agent dit du jeu (nom, façon d'arrêter) : la fenêtre reprend la phrase de la fiche à la place de la phrase
   * générale (BR-JEU-004). `null` : pas un serveur de jeu (adopté, déployé à la main), ou la lecture a échoué : la
   * phrase générale reste, l'arrêt reste possible (BR-LIFE-008).
   */
  game: GameView | null;
}

/**
 * La séquence d'un arrêt ou d'un redémarrage de serveur de jeu que la fenêtre de suivi porte (HRT-104, BR-JEU-003) : la
 * même forme que le suivi d'une suppression. `opId` `null` : le lien est tombé avant la réponse, le résultat n'est pas
 * connu (BR-JEU-012).
 */
export interface GameSequence {
  opId: string | null;
  /** Le nom du service au lancement (la liste peut le perdre). */
  name: string;
  gesture: "stop" | "restart";
  game: GameView;
  unknown: boolean;
}

/**
 * Le parcours de pilotage d'un service, tenu par la page (HRT-65, BR-LIFE-007, 016, 017, 022) :
 *
 * - arrêter et redémarrer ouvrent TOUJOURS la fenêtre de confirmation (l'effet, les avertissements du service, puis
 *   le mot de passe seulement si l'agent le demande : jamais en état normal, HRT-194) ; pour un serveur de jeu, l'effet est
 *   celui de sa fiche (HRT-104) et l'acceptation ouvre la séquence « Sauvegarde du monde » puis « Arrêt » ;
 * - démarrer n'ouvre la fenêtre que si l'agent a quelque chose à demander (mot de passe) ; sinon, en état normal
 *   avec la clé de ce poste, l'ordre part tout de suite ;
 * - annuler la fenêtre ne change rien et n'envoie rien (BR-LIFE-017) ;
 * - l'agent répond « accepté » : l'issue arrive par le suivi des opérations (HRT-84), jamais ici.
 *
 * Le service est suivi par sa CLÉ dans la liste vivante : si son état change sous la fenêtre, la fenêtre le dit.
 */
export function useServiceLifecycle(
  serverId: () => string,
  services: () => readonly ServiceInfo[],
) {
  const toasts = useToastsStore();
  const waitingSettings = useGameSettingsPendingStore();
  const actions = useLifecycleActions(serverId);
  /** Le dernier geste demandé : la fenêtre garde son sujet le temps de se fermer. */
  const pending = ref<PendingGesture | null>(null);
  const open = ref(false);
  /** La séquence d'un arrêt de jeu que la fenêtre de suivi porte ; `null` : aucune. */
  const sequence = ref<GameSequence | null>(null);
  const sequenceOpen = ref(false);
  const follow = useOperationFollow(serverId, () => sequence.value?.opId ?? null);
  // La lecture de l'état de confirmation pour un démarrage direct (jamais gardée au-delà de l'appel).
  const startCheck = useReauth(serverId, () => "service_start");
  const starting = ref(false);
  /** La lecture de ce que l'agent dit du jeu est en cours : un seul clic à la fois. */
  const reading = ref(false);

  /** Le service visé, tel qu'il est maintenant dans la liste ; `null` : il a disparu. */
  const current = computed<ServiceInfo | null>(() =>
    pending.value
      ? (services().find((service) => service.key === pending.value?.service.key) ?? null)
      : null,
  );

  /** Les refus qui ne demandent aucun geste de plus : un message, et la fenêtre se ferme (BR-LIFE-014, 015). */
  function settle(report: ActReport, service: ServiceInfo): ActReport {
    if (report.kind !== "refused" || !report.refusal) return report;
    switch (report.refusal.kind) {
      case "already_in_state": {
        const refusal = report.refusal as { state?: ServiceInfo["state"] | null };
        toasts.push({ kind: "info", message: alreadyText(refusal.state, service) });
        return { kind: "done" };
      }
      case "service_not_found":
        toasts.push({ kind: "error", message: refusalText(report.refusal, service) });
        return { kind: "done" };
      default:
        return report;
    }
  }

  /** Un démarrage ou un redémarrage accepté applique les réglages enregistrés : le repère « À appliquer » s'efface (HRT-109). */
  function launched(report: ActReport, service: ServiceInfo, gesture: Gesture): void {
    if (report.kind === "done" && gesture !== "stop") {
      waitingSettings.clear(serverId(), service.key);
    }
  }

  /** Lance l'acte (appelé par la fenêtre, avec le mot de passe saisi ou `null` sous l'élévation). */
  async function perform(adminPassword: string | null): Promise<ActReport> {
    const target = pending.value;
    if (!target || !open.value) return { kind: "failed" };
    const report = await actions.perform(target.service.key, target.gesture, adminPassword);
    launched(report, target.service, target.gesture);
    followGame(report, target);
    return settle(report, target.service);
  }

  /**
   * Un arrêt ou un redémarrage de serveur de jeu accepté (ou dont le résultat est inconnu) : la fenêtre de suivi prend le
   * relais et suit l'opération elle-même, son issue ne passe pas par une notification tant qu'elle est ouverte.
   */
  function followGame(report: ActReport, target: PendingGesture): void {
    if (!target.game || target.gesture === "start") return;
    const made = {
      name: target.service.name,
      gesture: target.gesture,
      game: target.game,
    };
    if (report.kind === "unknown") {
      sequence.value = { ...made, opId: null, unknown: true };
      sequenceOpen.value = true;
      return;
    }
    const id = actions.lastActionId.value;
    if (report.kind !== "done" || id === null) return;
    sequence.value = { ...made, opId: id, unknown: false };
    follow.begin(id);
    sequenceOpen.value = true;
  }

  /**
   * Rouvre la séquence d'un arrêt de jeu en cours (« Voir » de la bande, constat 7 de Nora) : l'opération existe déjà, la
   * fenêtre la suit de nouveau. Sans effet pour un service qui n'est pas un serveur de jeu, ou si la séquence est déjà là.
   */
  async function resumeSequence(service: ServiceInfo, operation: ServiceOperation): Promise<void> {
    if (operation.kind === "start" || sequenceOpen.value) return;
    if (sequence.value?.opId === operation.id) {
      sequenceOpen.value = true;
      follow.begin(operation.id);
      return;
    }
    const game = await readGame(service);
    if (!game) return;
    sequence.value = {
      opId: operation.id,
      name: service.name,
      gesture: operation.kind === "restart" ? "restart" : "stop",
      game,
      unknown: false,
    };
    follow.begin(operation.id);
    sequenceOpen.value = true;
  }

  /** « Fermer » : l'issue est dite, ou la séquence continue sur le serveur et la bande la porte. */
  function closeSequence(): void {
    if (follow.finished.value) follow.settle();
    else follow.release();
    sequenceOpen.value = false;
  }

  /**
   * Lit ce que l'agent dit du jeu avant d'ouvrir la fenêtre d'un arrêt ou d'un redémarrage. Un service qui n'est pas un
   * serveur de jeu, une lecture qui échoue, un agent plus ancien : `null`, la fenêtre générale s'ouvre (l'arrêt n'est
   * jamais refusé, BR-LIFE-008).
   */
  async function readGame(service: ServiceInfo): Promise<GameView | null> {
    if (service.origin !== "hearth" || service.adopted !== null) return null;
    try {
      const read = await getLinkBridge().readGameView(serverId(), service.key);
      return read.kind === "found" ? read.view : null;
    } catch (error) {
      if (!failureOf(error)) logUiError(error, "gameView:read");
      return null;
    }
  }

  /**
   * Demande un geste. Arrêter et redémarrer : la fenêtre s'ouvre. Démarrer : la fenêtre ne s'ouvre que si l'agent
   * demande quelque chose (BR-LIFE-016, grille de confiance HRT-194), sinon l'ordre part directement.
   */
  async function request(service: ServiceInfo, gesture: Gesture): Promise<void> {
    if (gesture !== "start") {
      if (reading.value) return;
      reading.value = true;
      try {
        show(service, gesture, await readGame(service));
      } finally {
        reading.value = false;
      }
      return;
    }
    if (starting.value) return;
    starting.value = true;
    try {
      await startCheck.load();
      const nothingToAsk =
        startCheck.ready.value &&
        startCheck.supported.value &&
        !startCheck.keyMissing.value &&
        !startCheck.needsPassword.value;
      if (!nothingToAsk) {
        show(service, gesture);
        return;
      }
      const sent = await actions.perform(service.key, "start", null);
      launched(sent, service, "start");
      const report = settle(sent, service);
      // L'état de confiance a changé entre la lecture et l'ordre (normal vers alerte, ou délai fermé) : la fenêtre
      // s'ouvre avec le champ, le geste n'est pas perdu.
      if (report.kind === "refused" && report.refusal?.kind === "password_required") {
        show(service, gesture);
        return;
      }
      if (report.kind === "refused" && report.refusal) {
        toasts.push({ kind: "error", message: startRefusal(report.refusal, service) });
      }
    } finally {
      starting.value = false;
    }
  }

  function startRefusal(refusal: { kind: string }, service: ServiceInfo): string {
    if (refusal.kind === "wrong_password" || refusal.kind === "too_many_attempts") {
      return t("lifecycle.refusedOther");
    }
    return refusalText(refusal, service);
  }

  function show(service: ServiceInfo, gesture: Gesture, game: GameView | null = null): void {
    pending.value = { service, gesture, game };
    open.value = true;
  }

  /** Ferme la fenêtre sans rien envoyer ni rien inscrire (BR-LIFE-017). */
  function cancel(): void {
    open.value = false;
  }

  return {
    pending,
    current,
    open,
    busy: computed(() => actions.busy.value || starting.value || reading.value),
    sequence,
    sequenceOpen,
    /** L'opération de la séquence telle qu'elle court (étape, décision attendue). */
    sequenceOperation: follow.operation,
    /** L'issue de la séquence, une fois terminée. */
    sequenceEnded: computed(() => follow.finished.value?.operation ?? null),
    closeSequence,
    resumeSequence,
    request,
    perform,
    cancel,
  };
}
