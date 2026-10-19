import { isHearthUsername } from "@/accounts/name";
import { safeText } from "@/audit/text";
import type { IconName } from "@/components/atoms/HIcon.vue";
import { type MessageKey, t } from "@/i18n";
import type { LifecycleGesture, LifecycleRefusal, ServiceInfo, ServiceState } from "@/link";
import { launchedBy } from "@/serviceOps/messages";
import { ENGINE_STATE_KEYS, STATE_KEYS } from "@/services/format";
import { busyText, type PilotLock } from "@/services/pilot";
import { canAct, type RightsGate } from "@/services/rights";

/**
 * Démarrer, arrêter, redémarrer un service (HRT-65, BR-LIFE-003, 007, 008, 013, 022) : des fonctions pures, sans
 * E/S. L'agent reste l'arbitre à chaque demande ; ceci dit d'avance, honnêtement, quels boutons existent, pourquoi
 * l'un est inactif, ce que la confirmation annonce et ce que l'agent a refusé. Aucun texte ne vient de l'agent : il
 * envoie des CODES, l'interface les met en phrases. Un nom de service ou d'unité venu du serveur est un texte
 * inoffensif (`safeText`), jamais du HTML.
 */

/** Le geste de pilotage : celui du pont (`LifecycleGesture`), seule définition. */
export type Gesture = LifecycleGesture;

/** L'icône de chaque geste (boutons du détail, menu de la ligne). */
export const ICONS: Record<Gesture, IconName> = {
  start: "play",
  stop: "stop",
  restart: "refresh",
};

/** Les gestes que l'état propose (BR-LIFE-003) : un geste que l'état ne propose pas n'existe pas, il n'est pas grisé. */
const OFFERED: Partial<Record<ServiceState, readonly Gesture[]>> = {
  running: ["stop", "restart"],
  stopped: ["start"],
  error: ["start"],
  restart_loop: ["stop", "restart"],
};

export function offeredGestures(state: ServiceState): readonly Gesture[] {
  return OFFERED[state] ?? [];
}

/**
 * Un service créé hors de Hearth et pas adopté (BR-LIFE-024) : l'agent le refuse comme une clé inconnue. FIX:01M5A5VG2H1ESNN212NNMT2PP0
 * Garde défensive : l'agent ne publie jamais un tel service (BR-ADOPT-001, `visibility_of` → caché), seul le
 * simulateur en montre un. Ne pas en déduire qu'un service externe de la liste peut être non adopté.
 */
export function isNotAdopted(service: Pick<ServiceInfo, "origin" | "adopted">): boolean {
  return service.origin === "external" && service.adopted === null;
}

/** Les états où une action est déjà en cours (transition) : aucun geste, et le service le dit. */
const TRANSITIONS: readonly ServiceState[] = ["starting", "stopping", "deploying", "updating"];

/** Pourquoi aucun geste n'est proposé : une action tient le service, ou Hearth ne le pilote pas dans cet état. */
export type NoGesture = "transition" | "not-drivable";

export function noGestureReason(state: ServiceState): NoGesture | null {
  if (offeredGestures(state).length > 0) return null;
  return TRANSITIONS.includes(state) ? "transition" : "not-drivable";
}

export interface Controls {
  /** Les gestes montrés (jamais ceux que l'état exclut). */
  gestures: readonly Gesture[];
  /** Les boutons sont visibles mais inactifs (droit, moteur, verrou) ; le lien coupé est géré par `needs-link`. */
  disabled: boolean;
  /**
   * La phrase de raison écrite sous le groupe, hors droit : le droit s'explique par `ServiceRightReason` (une seule
   * fois, `BR-GRANT-014`) ; le verrou et le moteur par cette phrase.
   */
  reason: string | null;
  /** Aucun geste n'est proposé : la phrase qui le dit à leur place (BR-LIFE-003). */
  note: string | null;
  /** Le compte qui tient le verrou, « Lancée par {compte}. » (BR-LIFE-013). */
  by: string | null;
}

/**
 * Ce que le détail d'un service montre pour le pilotage (BR-LIFE-003, 013, BR-GRANT-014, BR-ENGINE-014). L'état
 * d'abord : un geste qu'il exclut est absent. Ensuite ce qui bloque un geste que l'état propose : le droit, le
 * verrou d'une action, le moteur (visibles, désactivés, avec la raison écrite).
 */
export function controlsOf(input: {
  service: ServiceInfo;
  gate: RightsGate;
  lock: PilotLock;
  engineReady: boolean;
}): Controls {
  const { service, gate, lock, engineReady } = input;
  const none = (note: string | null): Controls => ({
    gestures: [],
    disabled: false,
    reason: null,
    note,
    by: null,
  });
  // FIX:01M5A5VG2H1ESNN212NNMT2PP0 (BR-LIFE-024) : l'agent refuserait tout geste sur un service non adopté ; aucun n'est proposé, la raison est dite.
  if (isNotAdopted(service)) return none(t("lifecycle.notAdopted"));
  const absent = noGestureReason(service.state);
  if (absent === "not-drivable") return none(t("lifecycle.notDrivable"));
  if (absent === "transition") return none(busyText(lock));
  const gestures = offeredGestures(service.state);
  // Une action tient le service, quel que soit l'état réel affiché : les boutons sont désactivés (BR-SVC-031).
  if (lock.locked) {
    return { gestures, disabled: true, reason: busyText(lock), note: null, by: lock.by };
  }
  if (!canAct(gate)) {
    return { gestures, disabled: true, reason: rightsReason(gate), note: null, by: null };
  }
  if (!engineReady) {
    return {
      gestures,
      disabled: true,
      reason: t("lifecycle.engineNotReady"),
      note: null,
      by: null,
    };
  }
  return { gestures, disabled: false, reason: null, note: null, by: null };
}

/**
 * La phrase écrite sous le groupe de boutons quand le droit les bloque (BR-GRANT-014) : jamais une infobulle seule.
 * Pendant la lecture du droit (5 s au plus) ou quand l'état du moteur parle déjà (BR-GRANT-015), rien n'est dit.
 */
function rightsReason(gate: RightsGate): string | null {
  switch (gate.kind) {
    case "denied":
      return t("rights.reason");
    case "old-agent":
      return t("rights.oldAgent");
    case "failed":
      return t("rights.readFailed");
    default:
      return null;
  }
}

// ── Les textes ───────────────────────────────────────────────────────────────────────────────────

export function stateName(state: ServiceState): string {
  return t(STATE_KEYS[state]);
}

export const LABEL_KEYS: Record<Gesture, MessageKey> = {
  start: "lifecycle.start",
  stop: "lifecycle.stop",
  restart: "lifecycle.restart",
};

const TITLE_KEYS: Record<Gesture, MessageKey> = {
  start: "lifecycle.titleStart",
  stop: "lifecycle.titleStop",
  restart: "lifecycle.titleRestart",
};

const SUBMIT_KEYS: Record<Gesture, MessageKey> = {
  start: "lifecycle.submitStart",
  stop: "lifecycle.submitStop",
  restart: "lifecycle.submitRestart",
};

export function titleOf(gesture: Gesture, name: string): string {
  return t(TITLE_KEYS[gesture], { name });
}

export function submitLabel(gesture: Gesture): string {
  return t(SUBMIT_KEYS[gesture]);
}

/** Le nom d'un service pour une phrase : texte du serveur rendu inoffensif. */
export function nameOf(service: Pick<ServiceInfo, "name">): string {
  return safeText(service.name);
}

/** Le délai d'arrêt écrit (« 120 secondes »), ou `null` quand l'agent ne l'a pas dit : jamais une valeur inventée. */
export function graceSeconds(service: Pick<ServiceInfo, "graceS">): string | null {
  const seconds = service.graceS;
  if (seconds === null || !Number.isFinite(seconds) || seconds < 0) return null;
  return t(seconds === 1 ? "serviceDetail.graceOne" : "serviceDetail.grace", { n: seconds });
}

/** L'effet d'un arrêt ou d'un redémarrage (BR-LIFE-007), avec le délai d'arrêt réel du service. */
export function effectText(
  gesture: "stop" | "restart",
  service: Pick<ServiceInfo, "graceS">,
): string {
  const grace = graceSeconds(service);
  if (gesture === "stop") {
    return grace === null
      ? t("lifecycle.effectStopNoDelay")
      : t("lifecycle.effectStop", { delay: grace });
  }
  return grace === null
    ? t("lifecycle.effectRestartNoDelay")
    : t("lifecycle.effectRestart", { delay: grace });
}

export interface Warning {
  tone: "warn" | "info";
  text: string;
  /** Pour les tests et le style : quel avertissement c'est. */
  id: "adopted" | "always" | "unit";
}

/**
 * Les avertissements propres au service, sous l'effet (BR-LIFE-007, 008) : un service créé hors de Hearth
 * (adopté), toujours visible, jamais replié ; une règle « Redémarre toujours » ; un service géré par le système du
 * serveur (qui remplace la règle précédente : c'est le système qui le relance). Hearth ne refuse jamais l'arrêt.
 */
export function warningsOf(
  service: Pick<ServiceInfo, "adopted" | "restart" | "driver" | "unit">,
): Warning[] {
  const out: Warning[] = [];
  if (service.adopted !== null) {
    out.push({ tone: "warn", id: "adopted", text: t("lifecycle.warnAdopted") });
  }
  if (service.driver === "supervisor") {
    const unit = service.unit === null ? null : safeText(service.unit);
    out.push({
      tone: "info",
      id: "unit",
      text:
        unit === null || unit === ""
          ? t("lifecycle.warnUnitNoName")
          : t("lifecycle.warnUnit", { unit }),
    });
  } else if (service.restart === "always") {
    out.push({ tone: "info", id: "always", text: t("lifecycle.warnAlways") });
  }
  return out;
}

/**
 * Le service a changé sous la fenêtre de confirmation (BR-LIFE-022) : il a disparu, une autre action l'a pris, ou
 * son état n'offre plus ce geste. La fenêtre le dit, le bouton de confirmation est inactif, « Annuler » reste actif.
 */
export function staleNotice(
  gesture: Gesture,
  current: ServiceInfo | null,
  locked: boolean,
): string | null {
  if (current === null) return t("serviceDetail.gone");
  if (!offeredGestures(current.state).includes(gesture)) {
    return t("lifecycle.stateChanged", { state: stateName(current.state) });
  }
  if (locked) return t("serviceOps.busy");
  return null;
}

/** Le compte qui tient le verrou, « Lancée par {compte}. », seulement pour un identifiant de compte valide. */
function by(account: string | null): string | null {
  return account !== null && isHearthUsername(account) ? launchedBy(account) : null;
}

/**
 * Le texte d'un refus de l'agent hors de la confirmation (BR-LIFE-013, 014, 015, 019). `null` : un refus que la
 * fenêtre ne dit pas elle-même (la confirmation se dit sous son champ).
 */
export function refusalText(
  refusal: { kind: string },
  service: Pick<ServiceInfo, "name"> & Partial<Pick<ServiceInfo, "origin" | "adopted">>,
): string {
  const shown = refusal as LifecycleRefusal;
  switch (shown.kind) {
    case "engine_unavailable": {
      const state =
        shown.engine && shown.engine !== "checking" ? ENGINE_STATE_KEYS[shown.engine] : undefined;
      return state
        ? t("lifecycle.refusedEngine", { state: t(state) })
        : t("lifecycle.refusedEngineNoState");
    }
    case "service_not_found":
      // FIX:01M5A5VG2H1ESNN212NNMT2PP0 : le refus d'un service non adopté ressemble à « inconnu » (BR-LIFE-024) ; ce n'est pas qu'il a disparu.
      return isNotAdopted({ origin: service.origin ?? "hearth", adopted: service.adopted ?? null })
        ? t("lifecycle.notAdopted")
        : t("serviceDetail.gone");
    case "service_busy": {
      const who = by(shown.by);
      return who === null ? t("serviceOps.busy") : `${t("serviceOps.busy")} ${who}`;
    }
    case "already_in_state":
      return alreadyText(shown.state, service);
    case "not_drivable":
      return t("lifecycle.notDrivable");
    default:
      return t("lifecycle.refusedOther");
  }
}

/** « Le service {nom} est déjà arrêté / en marche. » (BR-LIFE-014) : l'état dit par l'agent, jamais deviné. */
export function alreadyText(
  state: ServiceState | null | undefined,
  service: Pick<ServiceInfo, "name">,
): string {
  const running = state === "running" || state === "starting" || state === "restart_loop";
  return t(running ? "lifecycle.alreadyRunning" : "lifecycle.alreadyStopped", {
    name: nameOf(service),
  });
}
