import { computed, reactive, ref, watch } from "vue";
import { useDeployCheck } from "@/composables/useDeployCheck";
import type { DeployFlowSource } from "@/composables/useDeployFlow";
import { useEngineCapacity } from "@/composables/useEngineCapacity";
import { useFicheDetail } from "@/composables/useFicheDetail";
import type { PortVerdict } from "@/deploy/messages";
import { portVerdict } from "@/deploy/messages";
import { nameIssue } from "@/deploy/rules";
import { answerIssues, visibleQuestions } from "@/fiche/answers";
import type { FicheDraft } from "@/fiche/draft";
import { toInstallRequest } from "@/fiche/draft";
import { diskVerdict, memoryNotices, minimumVerdicts, placement, tooSmall } from "@/fiche/fit";
import { ficheFormRefusal } from "@/fiche/messages";
import { proposedName } from "@/fiche/name";
import {
  duplicateHostPorts,
  type HostPortIssue,
  parseHostPort,
  publishedPorts,
  readableHostPorts,
  startingHostPort,
} from "@/fiche/ports";
import { type DeployCheckInput, type DeployRefusal, getLinkBridge, type ServiceInfo } from "@/link";
import type { Resources } from "@/resources";
import { useFicheInstallStore } from "@/stores/ficheInstall";
import { useServicesStore } from "@/stores/services";

/**
 * L'installation d'un serveur de jeu depuis une fiche (HRT-102) : la fiche lue, le brouillon en mémoire, les défauts de
 * forme, les constats du serveur en direct (nom, ports, place), les gestes de saisie et les deux premières étapes. Tout
 * est INDICATIF : le serveur refait chaque contrôle au lancement (BR-DEPLOY-032). Le parcours qui suit (récapitulatif,
 * mot de passe, progression, issue) est celui du déploiement : `useDeployFlow`, par `flowSource`.
 *
 * - une question au serveur ne porte JAMAIS une réponse de la fiche (BR-DEPLOY-033) : seulement le nom, l'image de la
 *   fiche et les ports ;
 * - une erreur de forme ne s'affiche qu'une fois le champ quitté (ou la page validée) : jamais en rouge pendant la
 *   frappe d'un champ pas encore quitté ;
 * - la saisie ne va ni dans l'adresse, ni sur le disque (BR-FICHE-034).
 */
export function useFicheInstall(
  serverId: () => string,
  ficheId: () => string,
  connected: () => boolean,
  enabled: () => boolean,
) {
  const store = useFicheInstallStore();
  const services = useServicesStore();
  const { detail, phase, reload } = useFicheDetail(serverId, ficheId, connected);

  const takenNames = (): string[] =>
    (services.of(serverId())?.services ?? []).map((service: ServiceInfo) => service.name);

  // Le brouillon : celui de cette fiche si elle n'a pas changé, sinon un brouillon neuf.
  watch(
    detail,
    (read) => {
      if (read) store.begin(serverId(), read, () => proposedName(read.card.name, takenNames()));
    },
    { immediate: true },
  );
  const draft = computed<FicheDraft | null>(() =>
    detail.value ? (store.of(serverId(), ficheId()) ?? null) : null,
  );

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

  // ── Les défauts de forme ─────────────────────────────────────────────────────────────────────
  const questions = computed(() =>
    detail.value && draft.value ? visibleQuestions(detail.value, draft.value.variant) : [],
  );
  const answerProblems = computed(() =>
    draft.value ? answerIssues(questions.value, draft.value.answers) : new Map(),
  );
  const published = computed(() =>
    detail.value && draft.value
      ? publishedPorts(detail.value.ports, draft.value.optionalPorts, draft.value.variant)
      : [],
  );
  const duplicates = computed(() =>
    draft.value ? duplicateHostPorts(published.value, draft.value.hostPorts) : new Set<string>(),
  );
  /** Le port saisi de chaque port publié, jugé dans sa forme ; un doublon l'est aussi. */
  const portProblems = computed(() => {
    const found = new Map<string, HostPortIssue | "duplicate">();
    if (!draft.value) return found;
    for (const port of published.value) {
      const parsed = parseHostPort(draft.value.hostPorts[port.id] ?? startingHostPort(port));
      if ("issue" in parsed) found.set(port.id, parsed.issue);
      else if (duplicates.value.has(port.id)) found.set(port.id, "duplicate");
    }
    return found;
  });
  const nameProblem = computed(() => (draft.value ? nameIssue(draft.value.name) : null));
  /** Au moins un défaut de forme sur la page « Réponses ». */
  const answersInvalid = computed(
    () =>
      nameProblem.value !== null || answerProblems.value.size > 0 || portProblems.value.size > 0,
  );

  // ── Les constats du serveur ──────────────────────────────────────────────────────────────────
  const checkInput = computed<DeployCheckInput | null>(() => {
    const read = detail.value;
    const current = draft.value;
    if (!read || !current) return null;
    const ports = readableHostPorts(published.value, current.hostPorts).map(({ deploy }) => deploy);
    if (nameIssue(current.name) !== null && ports.length === 0) return null;
    return { name: current.name, image: read.image.repository, tag: read.image.tag, ports };
  });
  const checks = useDeployCheck(serverId, enabled, checkInput);
  const check = checks.check;

  /** Ce que le serveur dit de chaque port publié, par identifiant de port de la fiche. */
  const portVerdicts = computed(() => {
    const map = new Map<string, PortVerdict>();
    if (!draft.value) return map;
    for (const { id, deploy } of readableHostPorts(published.value, draft.value.hostPorts)) {
      const found = check.value?.ports.find(
        (item) => item.hostPort === deploy.hostPort && item.protocol === deploy.protocol,
      );
      map.set(id, portVerdict(found));
    }
    return map;
  });
  const portsWaiting = computed(() => checks.checking.value && check.value === null);

  // ── Les bornes de la machine, les curseurs ───────────────────────────────────────────────────
  const limits = useEngineCapacity(serverId, enabled);
  const capacity = limits.capacity;
  // La fiche et les bornes de la machine arrivent dans un ordre quelconque : les curseurs se placent dès que les deux
  // sont là, et un réglage déjà fait n'est jamais écrasé.
  watch(
    [draft, detail, capacity],
    () => {
      const current = draft.value;
      const fiche = detail.value;
      const read = capacity.value;
      if (current && fiche && read && current.resources === null) {
        current.resources = placement(fiche, read).resources;
      }
    },
    { immediate: true },
  );

  const minimums = computed(() =>
    detail.value ? minimumVerdicts(detail.value, capacity.value) : [],
  );
  /** Le constat « machine trop petite » (minimum mesuré) ou `null`. */
  const tooSmallVerdict = computed(() => tooSmall(minimums.value));
  const placed = computed(() =>
    detail.value && capacity.value ? placement(detail.value, capacity.value) : null,
  );
  const disk = computed(() =>
    detail.value
      ? diskVerdict(detail.value, check.value?.disk?.freeBytes ?? null)
      : { kind: "unknown" as const },
  );
  const notices = computed(() =>
    detail.value && capacity.value
      ? memoryNotices(detail.value, capacity.value, draft.value?.resources?.memoryBytes ?? null)
      : [],
  );

  // ── Les étapes ───────────────────────────────────────────────────────────────────────────────
  const step = ref<1 | 2>(1);
  /** Passe à l'étape « Ressources » si la page « Réponses » n'a aucun défaut ; sinon les montre tous. */
  function next(): boolean {
    touchAll();
    if (answersInvalid.value) return false;
    step.value = 2;
    return true;
  }
  function back(): void {
    step.value = 1;
  }

  // ── Les gestes de saisie ─────────────────────────────────────────────────────────────────────
  function edit(change: (current: FicheDraft) => void): void {
    const current = draft.value;
    if (!current) return;
    current.edited = true;
    change(current);
  }
  function setName(name: string): void {
    edit((current) => {
      current.name = name;
    });
  }
  function setVariant(variant: string): void {
    edit((current) => {
      current.variant = variant;
    });
  }
  function setAnswer(id: string, value: string): void {
    edit((current) => {
      current.answers[id] = value;
    });
  }
  function setHostPort(id: string, value: string): void {
    edit((current) => {
      current.hostPorts[id] = value;
    });
  }
  function setOptionalPort(id: string, on: boolean): void {
    edit((current) => {
      const index = current.optionalPorts.indexOf(id);
      if (on && index < 0) current.optionalPorts.push(id);
      if (!on && index >= 0) current.optionalPorts.splice(index, 1);
    });
  }
  function setResources(resources: Resources): void {
    edit((current) => {
      current.resources = resources;
    });
  }

  // ── Les contrats acceptés ────────────────────────────────────────────────────────────────────
  /** Les contrats que l'utilisateur vient d'accepter par un geste (jamais cochés d'avance, BR-FICHE-058). */
  const agreed = ref<string[]>([]);

  /** Efface la saisie de la mémoire (lancement, abandon) et remet la page à neuf. */
  function wipe(): void {
    store.wipe(serverId(), ficheId());
    resetTouched();
    checks.reset();
    agreed.value = [];
    step.value = 1;
  }

  /** Le parcours de déploiement vu depuis cette fiche (`useDeployFlow`). */
  const flowSource: DeployFlowSource = {
    validate() {
      touchAll();
      return answersInvalid.value || tooSmallVerdict.value !== null;
    },
    refreshCheck: () => checks.refresh(),
    prepare(accept) {
      const fiche = detail.value;
      const current = draft.value;
      if (!fiche || !current) return null;
      const request = toInstallRequest(fiche, current, { accept, agreements: agreed.value });
      if (!request) return null;
      return {
        name: request.name,
        run: (id, adminPassword) => getLinkBridge().installFiche(id, request, adminPassword),
      };
    },
    wipe,
    refusal: (refused: DeployRefusal) => ficheFormRefusal(refused, detail.value),
  };

  return {
    detail,
    phase,
    reload,
    draft,
    questions,
    answerProblems,
    published,
    portProblems,
    nameProblem,
    answersInvalid,
    touch,
    touchAll,
    shown,
    check,
    checking: checks.checking,
    refreshCheck: checks.refresh,
    portVerdicts,
    portsWaiting,
    capacity,
    capacityFailed: limits.failed,
    loadCapacity: limits.load,
    minimums,
    tooSmallVerdict,
    placed,
    disk,
    notices,
    step,
    next,
    back,
    setName,
    setVariant,
    setAnswer,
    setHostPort,
    setOptionalPort,
    setResources,
    agreed,
    wipe,
    flowSource,
  };
}

export type FicheInstallState = ReturnType<typeof useFicheInstall>;
