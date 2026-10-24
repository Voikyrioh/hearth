import { computed, reactive, ref, watch } from "vue";
import { useDeployCheck } from "@/composables/useDeployCheck";
import { useEngineCapacity } from "@/composables/useEngineCapacity";
import { type Draft, type EnvRow, isFilled, movedVersion, type PortRow } from "@/deploy/draft";
import { type FormIssues, formIssues, readablePorts, toCheckInput } from "@/deploy/form";
import { type PortVerdict, portVerdict } from "@/deploy/messages";
import type { DeployProtocol } from "@/link";
import { startingResources } from "@/resources";
import { useDeployFormStore } from "@/stores/deployForm";

export { CHECK_DELAY_MS } from "@/composables/useDeployCheck";

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
  /** Une saisie a eu lieu depuis l'ouverture du formulaire (même effacée ensuite). */
  const started = ref(false);
  watch(
    () => isFilled(draft.value),
    (filled) => {
      if (filled) started.value = true;
    },
    { immediate: true, flush: "sync" },
  );
  function touch(key: string): void {
    // Un formulaire vierge ne se plaint de rien en quittant un champ : l'erreur « requis » qui apparaîtrait sous le nom
    // décalerait les boutons sous le clic en cours (« Ajouter un port » perdu au premier essai). Elle viendra à la
    // première saisie ou à « Déployer ».
    if (!started.value) return;
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
    started.value = isFilled(draft.value);
    for (const key of Object.keys(touched)) delete touched[key];
  }

  // ── Les constats du serveur (partagés avec l'installation d'une fiche) ──────────────────────
  const input = computed(() => toCheckInput(draft.value));
  const checks = useDeployCheck(serverId, enabled, input);
  const check = checks.check;
  const checking = checks.checking;
  const refreshCheck = checks.refresh;

  /** Ce que le serveur dit d'une rangée de port. */
  const portVerdicts = computed(() => {
    const map = new Map<number, PortVerdict>();
    const answer = check.value;
    for (const port of readablePorts(draft.value)) {
      const found = answer?.ports.find(
        (item) => item.hostPort === port.hostPort && item.protocol === port.protocol,
      );
      map.set(port.id, portVerdict(found));
    }
    return map;
  });
  /** Une rangée de port attend encore la réponse du serveur. */
  const portsWaiting = computed(() => checking.value && check.value === null);

  // ── Les bornes de la machine ─────────────────────────────────────────────────────────────────
  // Les curseurs partent de la valeur de départ de BR-RES-039 ; un réglage déjà fait n'est pas écrasé.
  const limits = useEngineCapacity(serverId, enabled, (read) => {
    if (draft.value.resources === null) draft.value.resources = startingResources(read, null);
  });
  const capacity = limits.capacity;
  const capacityFailed = limits.failed;
  const loadCapacity = limits.load;

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
    checks.reset();
    movedImage.value = null;
  }

  return {
    draft,
    issues,
    touch,
    touchAll,
    shown,
    resetTouched,
    check,
    checking,
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
