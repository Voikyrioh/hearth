import { computed, onBeforeUnmount, reactive, ref, watch } from "vue";
import { type Draft, type EnvRow, movedVersion, type PortRow } from "@/deploy/draft";
import { type FormIssues, formIssues, readablePorts, toCheckInput } from "@/deploy/form";
import { type PortVerdict, portVerdict } from "@/deploy/messages";
import { logUiError } from "@/errors/report";
import { type DeployCheck, type DeployProtocol, getLinkBridge } from "@/link";
import { type EngineCapacity, startingResources } from "@/resources";
import { useDeployFormStore } from "@/stores/deployForm";

/** Le délai après la dernière frappe avant de demander au serveur (les constats sont indicatifs, BR-DEPLOY-032). */
export const CHECK_DELAY_MS = 400;

/**
 * Le formulaire « Déployer un service » d'un serveur (HRT-69) : le brouillon en mémoire, les défauts de forme, les
 * constats du serveur en direct (nom, ports, image, espace disque) et les gestes sur les rangées. Tout est INDICATIF :
 * le serveur refait chaque contrôle au lancement avec la même sévérité (BR-DEPLOY-032). Les constats de CONFLIT (nom
 * pris, port tenu, plus petit port libre) ne sont jamais recalculés ici : ils viennent de l'agent (BR-DEPLOY-008, 009).
 *
 * - une question au serveur ne porte JAMAIS un secret : ni variable, ni commande (BR-DEPLOY-033) ;
 * - une réponse en retard sur une plus récente est écartée ;
 * - une erreur de forme ne s'affiche qu'une fois le champ quitté (ou le formulaire soumis) : jamais en rouge pendant
 *   la frappe d'un champ pas encore quitté ; les constats du serveur sur le nom et les ports sont immédiats.
 */
export function useDeployForm(serverId: () => string, enabled: () => boolean) {
  const store = useDeployFormStore();
  const draft = computed<Draft>(() => store.of(serverId()));
  const issues = computed<FormIssues>(() => formIssues(draft.value));

  // ── Les champs quittés ───────────────────────────────────────────────────────────────────────
  const touched = reactive<Record<string, boolean>>({});
  const submitted = ref(false);
  function touch(key: string): void {
    touched[key] = true;
  }
  function touchAll(): void {
    submitted.value = true;
  }
  /** Le défaut de ce champ doit-il être montré ? */
  function shown(key: string): boolean {
    return submitted.value || touched[key] === true;
  }
  function resetTouched(): void {
    submitted.value = false;
    for (const key of Object.keys(touched)) delete touched[key];
  }

  // ── Les constats du serveur ──────────────────────────────────────────────────────────────────
  const check = ref<DeployCheck | null>(null);
  const checkSent = ref<string | null>(null);
  const checking = ref(false);
  const checkFailed = ref(false);
  let ticket = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const input = computed(() => toCheckInput(draft.value));
  /** La consultation telle qu'elle partirait : sans secret, donc sûre à comparer. */
  const inputKey = computed(() => (input.value ? JSON.stringify(input.value) : null));

  /** Pose la question maintenant (sans attendre la fin de la frappe) et rend la réponse, `null` si elle n'a pas pu venir. */
  async function refreshCheck(): Promise<DeployCheck | null> {
    clearTimeout(timer);
    const asked = input.value;
    const key = inputKey.value;
    if (!asked || !enabled()) {
      check.value = null;
      checkSent.value = null;
      checking.value = false;
      return null;
    }
    const mine = ++ticket;
    checking.value = true;
    try {
      const answer = await getLinkBridge().checkDeployment(serverId(), asked);
      if (mine !== ticket) return null;
      check.value = answer;
      checkSent.value = key;
      checkFailed.value = false;
      return answer;
    } catch (error) {
      if (mine !== ticket) return null;
      // Une consultation qui échoue n'interdit rien : le serveur dira non au lancement s'il le faut.
      checkFailed.value = true;
      check.value = null;
      checkSent.value = null;
      logUiError(error, "deploy:check");
      return null;
    } finally {
      if (mine === ticket) checking.value = false;
    }
  }

  watch(inputKey, (key) => {
    clearTimeout(timer);
    if (key === null) {
      ticket += 1;
      check.value = null;
      checkSent.value = null;
      checking.value = false;
      return;
    }
    checking.value = true;
    timer = setTimeout(() => void refreshCheck(), CHECK_DELAY_MS);
  });
  // Le lien ou le moteur reviennent : on repose la question.
  watch(enabled, (now) => {
    if (now && inputKey.value !== null) void refreshCheck();
  });
  onBeforeUnmount(() => {
    clearTimeout(timer);
    ticket += 1;
  });

  /** Le constat du serveur est celui de ce que le formulaire dit MAINTENANT (pas d'une saisie plus ancienne). */
  const checkCurrent = computed(() => (checkSent.value === inputKey.value ? check.value : null));

  /** Ce que le serveur dit d'une rangée de port. */
  const portVerdicts = computed(() => {
    const map = new Map<number, PortVerdict>();
    const answer = checkCurrent.value;
    for (const port of readablePorts(draft.value)) {
      const found = answer?.ports.find(
        (item) => item.hostPort === port.hostPort && item.protocol === port.protocol,
      );
      map.set(port.id, portVerdict(found));
    }
    return map;
  });
  /** Une rangée de port attend encore la réponse du serveur. */
  const portsWaiting = computed(() => checking.value && checkCurrent.value === null);

  // ── Les bornes de la machine ─────────────────────────────────────────────────────────────────
  const capacity = ref<EngineCapacity | null>(null);
  const capacityFailed = ref(false);
  let capacityTicket = 0;

  async function loadCapacity(): Promise<void> {
    const mine = ++capacityTicket;
    try {
      const read = await getLinkBridge().readEngineCapacity(serverId());
      if (mine !== capacityTicket) return;
      capacity.value = read;
      capacityFailed.value = read === null;
      // Les curseurs partent de la valeur de départ de BR-RES-039 ; un réglage déjà fait n'est pas écrasé.
      if (read && draft.value.resources === null) {
        draft.value.resources = startingResources(read, null);
      }
    } catch (error) {
      if (mine !== capacityTicket) return;
      capacityFailed.value = true;
      logUiError(error, "deploy:capacity");
    }
  }
  // Un autre serveur a d'autres bornes (et d'autres constats) : rien de l'ancien ne reste.
  watch(serverId, () => {
    clearTimeout(timer);
    // Une réponse encore en vol de l'ancien serveur ne doit atterrir nulle part, même si la clé de consultation est
    // la même (deux brouillons de même nom).
    ticket += 1;
    capacityTicket += 1;
    capacity.value = null;
    capacityFailed.value = false;
    check.value = null;
    checkSent.value = null;
    checking.value = false;
    if (inputKey.value !== null && enabled()) void refreshCheck();
  });
  watch(
    [serverId, enabled],
    ([, on]) => {
      if (on && capacity.value === null) void loadCapacity();
    },
    { immediate: true },
  );

  // ── Les gestes sur les rangées ───────────────────────────────────────────────────────────────
  function addEnv(): number {
    const id = store.rowId();
    draft.value.env.push({ id, name: "", value: "", reveal: null });
    return id;
  }
  function removeEnv(id: number): void {
    const index = draft.value.env.findIndex((row) => row.id === id);
    if (index < 0) return;
    const row = draft.value.env[index];
    if (row) {
      row.name = "";
      row.value = "";
    }
    draft.value.env.splice(index, 1);
  }
  function updateEnv(id: number, patch: Partial<Pick<EnvRow, "name" | "value" | "reveal">>): void {
    const row = draft.value.env.find((item) => item.id === id);
    if (row) Object.assign(row, patch);
  }
  function updatePort(id: number, patch: Partial<Pick<PortRow, "host" | "container">>): void {
    const row = draft.value.ports.find((item) => item.id === id);
    if (row) Object.assign(row, patch);
  }
  function addPort(): number {
    const id = store.rowId();
    draft.value.ports.push({ id, host: "", container: "", protocol: "tcp" });
    return id;
  }
  function removePort(id: number): void {
    const index = draft.value.ports.findIndex((row) => row.id === id);
    if (index >= 0) draft.value.ports.splice(index, 1);
  }
  function setProtocol(id: number, protocol: DeployProtocol): void {
    const row = draft.value.ports.find((item) => item.id === id);
    if (row) row.protocol = protocol;
  }
  /** Remplace le port refusé par le port libre que l'agent a proposé (BR-DEPLOY-009) ; la consultation se refait. */
  function useSuggested(id: number, port: number): void {
    const row = draft.value.ports.find((item) => item.id === id);
    if (row) row.host = String(port);
  }

  /** La version écrite dans le champ de l'image passe dans le champ Version (BR-DEPLOY-005). */
  const movedImage = ref<string | null>(null);
  /** La note « La version a été déplacée » reste tant que l'image n'est pas retouchée. */
  const versionMoved = computed(
    () => movedImage.value !== null && draft.value.image === movedImage.value,
  );
  function moveVersion(): void {
    const moved = movedVersion(draft.value.image, draft.value.tag);
    if (!moved.moved) return;
    draft.value.image = moved.image;
    draft.value.tag = moved.tag;
    movedImage.value = moved.image;
  }

  /** Efface la saisie de la mémoire (lancement, abandon) et remet le formulaire à neuf. */
  function wipe(): void {
    store.wipe(serverId());
    resetTouched();
    check.value = null;
    checkSent.value = null;
    movedImage.value = null;
  }

  return {
    draft,
    issues,
    touch,
    touchAll,
    shown,
    resetTouched,
    check: checkCurrent,
    checking,
    checkFailed,
    refreshCheck,
    portVerdicts,
    portsWaiting,
    capacity,
    capacityFailed,
    loadCapacity,
    addEnv,
    removeEnv,
    updateEnv,
    updatePort,
    addPort,
    removePort,
    setProtocol,
    useSuggested,
    moveVersion,
    versionMoved,
    wipe,
  };
}

export type DeployFormState = ReturnType<typeof useDeployForm>;
