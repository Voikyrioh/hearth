import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { useDeployCheck } from "@/composables/useDeployCheck";
import type { DeployFlowSource } from "@/composables/useDeployFlow";
import { useEngineCapacity } from "@/composables/useEngineCapacity";
import { useFicheDetail } from "@/composables/useFicheDetail";
import type { ActReport } from "@/composables/useReauth";
import { useServerAction } from "@/composables/useServerAction";
import { useWorldImport } from "@/composables/useWorldImport";
import type { PortVerdict } from "@/deploy/messages";
import { portVerdict } from "@/deploy/messages";
import { nameIssue } from "@/deploy/rules";
import { answerIssues, extraSettings, visibleQuestions } from "@/fiche/answers";
import type { FicheDraft } from "@/fiche/draft";
import { toInstallRequest } from "@/fiche/draft";
import { diskVerdict, memoryNotices, minimumVerdicts, placement, tooSmall } from "@/fiche/fit";
import { ficheFormRefusal } from "@/fiche/messages";
import { proposedName } from "@/fiche/name";
import { allowedVariants, fileNameOf, packGenres, packIssue, readPack } from "@/fiche/pack";
import {
  duplicateHostPorts,
  type HostPortIssue,
  parseHostPort,
  publishedPorts,
  readableHostPorts,
  startingHostPort,
} from "@/fiche/ports";
import { ficheKnowsWorlds, hiddenByWorld, worldMissing } from "@/fiche/world";
import {
  type DeployCheckInput,
  type DeployRefusal,
  type DepositRefusal,
  getLinkBridge,
  type ServiceInfo,
} from "@/link";
import type { Resources } from "@/resources";
import { problemText } from "@/settings/messages";
import { useFicheInstallStore } from "@/stores/ficheInstall";
import { useServicesStore } from "@/stores/services";
import { useTransfersStore } from "@/stores/transfers";
import { depositRefusalText } from "@/transfers/messages";

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
  // Reprendre un monde : le monde porte son nom et ses réglages figés, la page ne les pose pas (HRT-106, BR-MONDE-014).
  const hidden = computed(() =>
    detail.value && draft.value
      ? hiddenByWorld(detail.value, draft.value.world)
      : new Set<string>(),
  );
  const questions = computed(() =>
    detail.value && draft.value
      ? visibleQuestions(detail.value, draft.value.variant).filter(
          (question) => !hidden.value.has(question.id),
        )
      : [],
  );
  const answerProblems = computed(() =>
    draft.value ? answerIssues(questions.value, draft.value.answers) : new Map(),
  );
  // Les réglages de « Voir tous les réglages » (HRT-109, BR-FICHE-040) : ceux qui ne sont pas des questions. Leur défaut de
  // forme ne compte que s'ils ont été touchés (sinon la valeur est celle que la fiche propose).
  const extras = computed(() =>
    detail.value && draft.value
      ? extraSettings(detail.value, draft.value.variant).filter(
          (setting) => !hidden.value.has(setting.question.id),
        )
      : [],
  );
  const extraProblems = computed(() => {
    const found = new Map<string, string>();
    const current = draft.value;
    if (!current) return found;
    for (const setting of extras.value) {
      const value = current.answers[setting.question.id];
      if (value === undefined) continue;
      const problem = problemText(setting, value);
      if (problem) found.set(setting.question.id, problem);
    }
    return found;
  });
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
  /** Reprendre un monde sans l'avoir choisi : la page ne continue pas (BR-MONDE-002). */
  const worldProblem = computed(() => (draft.value ? worldMissing(draft.value.world) : false));
  // ── Le pack de mods (HRT-124, HRT-125) ───────────────────────────────────────────────────────
  /** Les genres de pack que la fiche prend ; `null` : la fiche n'offre pas le dépôt d'un pack. */
  const packWay = computed(() => (detail.value ? packGenres(detail.value) : null));
  /** Ce qui empêche de continuer avec le pack (case cochée), ou `null`. */
  const packProblem = computed(() =>
    detail.value && draft.value
      ? packIssue(detail.value, draft.value.packWanted, draft.value.pack, draft.value.variant)
      : null,
  );
  const packBusy = ref(false);
  const transfers = useTransfersStore();
  const sendAction = useServerAction(() => serverId());
  /** La fenêtre « Envoyer le pack au serveur » (acte d'écriture : confirmation de l'agent) est ouverte. */
  const packSendOpen = ref(false);
  /** L'envoi du fichier s'est arrêté pour de bon (échec ou annulation) : le fichier est à choisir de nouveau. */
  const packSendFailed = ref(false);
  /** Le dernier refus à l'ouverture du dépôt (ce que la fenêtre dit sous le champ). */
  const packSendRefusal = ref<DepositRefusal | null>(null);
  /** L'envoi du pack choisi, tel que la liste des transferts le tient ; `null` : rien n'est parti. */
  const packTransfer = computed(() => {
    const deposit = draft.value?.pack?.deposit;
    return deposit ? (transfers.of(serverId()).find((item) => item.id === deposit) ?? null) : null;
  });
  let stopWatching: (() => void) | undefined;
  let stopped = false;
  onMounted(async () => {
    const stop = await transfers.watch();
    if (stopped) stop();
    else stopWatching = stop;
  });
  onBeforeUnmount(() => {
    stopped = true;
    stopWatching?.();
  });
  watch(
    () => packTransfer.value?.status,
    (status) => {
      const current = draft.value?.pack;
      if (!current || current.deposit === null || status === undefined) return;
      if (status === "done" && !current.delivered) {
        const deposit = current.deposit;
        edit((value) => {
          if (value.pack?.deposit === deposit) value.pack.delivered = true;
        });
        void readDeposit(deposit);
      } else if (status === "failed" || status === "cancelled") {
        packSendFailed.value = true;
        edit((value) => {
          value.pack = null;
        });
      }
    },
  );
  /** La lecture du pack a échoué (lien, droit) : le fichier est à choisir de nouveau. */
  const packReadFailed = ref(false);
  /** Au plus UN moteur permis pour ce genre de pack : il est choisi d'office (un pack Modrinth). */
  function alignEngine(): void {
    const current = draft.value;
    const read = detail.value;
    const pack = readPack(current?.pack ?? null);
    if (!current || !read || !pack) return;
    const allowed = allowedVariants(read, pack);
    const only = allowed[0];
    if (allowed.length === 1 && only !== undefined && current.variant !== only)
      current.variant = only;
  }
  function setPackWanted(on: boolean): void {
    edit((current) => {
      current.packWanted = on;
      if (!on) current.pack = null;
    });
    packSendFailed.value = false;
    packSendOpen.value = false;
    packReadFailed.value = false;
  }
  /** Demande à l'agent ce qu'est le pack déposé (une consultation, rien n'est rangé). */
  async function readDeposit(deposit: string): Promise<void> {
    packBusy.value = true;
    packReadFailed.value = false;
    try {
      const check = await getLinkBridge().inspectPack(serverId(), deposit);
      edit((current) => {
        if (current.pack?.deposit === deposit) current.pack.check = check;
      });
      alignEngine();
    } catch {
      packReadFailed.value = true;
      edit((current) => {
        current.pack = null;
      });
    } finally {
      packBusy.value = false;
    }
  }
  /** Choisit le fichier dans la boîte « Ouvrir » du système, puis ouvre la fenêtre qui l'envoie au serveur. */
  async function choosePack(): Promise<void> {
    if (packBusy.value) return;
    packSendFailed.value = false;
    packReadFailed.value = false;
    let path: string | null;
    try {
      path = await transfers.pick();
    } catch {
      packReadFailed.value = true;
      return;
    }
    if (path === null) return;
    edit((current) => {
      current.pack = {
        path,
        deposit: null,
        fileName: fileNameOf(path),
        delivered: false,
        check: null,
      };
    });
    packSendOpen.value = true;
  }
  /** Rouvre la fenêtre d'envoi (le compte l'avait fermée sans envoyer). */
  function openPackSend(): void {
    packSendOpen.value = true;
  }
  function closePackSend(): void {
    packSendOpen.value = false;
  }
  /** Ouvre l'envoi du fichier choisi (acte d'écriture, confirmé par l'agent) ; le fichier arrive par un transfert reprenable. */
  async function sendPack(adminPassword: string | null): Promise<ActReport> {
    const path = draft.value?.pack?.path;
    if (!path) return { kind: "failed" };
    packSendRefusal.value = null;
    const outcome = await sendAction.run(() => transfers.start(serverId(), path, adminPassword));
    if (outcome === null) return sendAction.failedReport();
    if (outcome.kind === "started") {
      const id = outcome.transfer.id;
      edit((current) => {
        if (current.pack?.path === path) current.pack.deposit = id;
      });
      return { kind: "done" };
    }
    if (outcome.kind === "unknown") return { kind: "unknown" };
    packSendRefusal.value = outcome.refusal;
    const refusal = outcome.refusal;
    return {
      kind: "refused",
      refusal:
        refusal.kind === "too_many_attempts"
          ? { kind: refusal.kind, retry_after_s: refusal.waitS }
          : { kind: refusal.kind },
    };
  }
  /** Ce que la fenêtre d'envoi dit d'un refus qui n'est pas celui de la confirmation. */
  function packSendRefusalText(): string | undefined {
    const refusal = packSendRefusal.value;
    return refusal === null ? undefined : depositRefusalText(refusal);
  }

  /** Au moins un défaut de forme sur la page « Réponses ». */
  const answersInvalid = computed(
    () =>
      nameProblem.value !== null ||
      worldProblem.value ||
      packProblem.value !== null ||
      answerProblems.value.size > 0 ||
      extraProblems.value.size > 0 ||
      portProblems.value.size > 0,
  );

  // ── Le monde repris (HRT-106) ────────────────────────────────────────────────────────────────
  const worldImport = useWorldImport(
    serverId,
    ficheId,
    enabled,
    () => detail.value !== null && ficheKnowsWorlds(detail.value),
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
    for (const { port, deploy } of readableHostPorts(published.value, draft.value.hostPorts)) {
      const found = check.value?.ports.find(
        (item) => item.hostPort === deploy.hostPort && item.protocol === deploy.protocol,
      );
      map.set(port.id, portVerdict(found));
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
  /** Remet un réglage de « Voir tous les réglages » à la valeur proposée par la fiche. */
  function resetAnswer(id: string): void {
    edit((current) => {
      delete current.answers[id];
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
  /** Reprendre un monde existant, ou créer un monde neuf. Revenir au monde neuf oublie le monde choisi. */
  function setResume(resume: boolean): void {
    edit((current) => {
      current.world.resume = resume;
      if (!resume) {
        current.world.folder = null;
        current.world.path = null;
        current.world.name = null;
        worldImport.clear();
      }
    });
  }
  /** Choisit le monde à reprendre et l'inspecte (une consultation, rien n'est écrit). */
  function chooseWorld(folder: string, path: string, name: string): void {
    edit((current) => {
      current.world.resume = true;
      current.world.folder = folder;
      current.world.path = path;
      current.world.name = name;
    });
    void worldImport.inspect({ folder, path });
  }
  function changeWorld(): void {
    edit((current) => {
      current.world.folder = null;
      current.world.path = null;
      current.world.name = null;
    });
    worldImport.clear();
  }
  function setKeepIfChanged(keep: boolean): void {
    edit((current) => {
      current.world.keepIfChanged = keep;
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
    worldImport.clear();
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
      // Le geste « Je comprends » de la version du monde est une case du récapitulatif : le parcours n'arrive ici qu'après.
      const request = toInstallRequest(fiche, current, {
        accept,
        agreements: agreed.value,
        worldVersionUnderstood: true,
      });
      if (!request) return null;
      return {
        name: request.name,
        run: (id, adminPassword) => getLinkBridge().installFiche(id, request, adminPassword),
      };
    },
    wipe,
    refusal: (refused: DeployRefusal) => {
      // Le pack refusé (ou disparu) est retiré de la zone d'attente de l'agent : il est à choisir de nouveau.
      if (refused.kind === "pack_unknown" || refused.kind === "pack_refused") {
        edit((current) => {
          current.pack = null;
        });
      }
      return ficheFormRefusal(refused, detail.value);
    },
  };

  return {
    detail,
    phase,
    reload,
    draft,
    questions,
    answerProblems,
    extras,
    extraProblems,
    published,
    portProblems,
    nameProblem,
    worldProblem,
    worldImport,
    packWay,
    packProblem,
    packBusy,
    serverId,
    packSendOpen,
    packSendFailed,
    packTransfer,
    openPackSend,
    closePackSend,
    sendPack,
    packSendRefusalText,
    packReadFailed,
    setPackWanted,
    choosePack,
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
    resetAnswer,
    setHostPort,
    setOptionalPort,
    setResume,
    chooseWorld,
    changeWorld,
    setKeepIfChanged,
    setResources,
    agreed,
    wipe,
    flowSource,
  };
}

export type FicheInstallState = ReturnType<typeof useFicheInstall>;
