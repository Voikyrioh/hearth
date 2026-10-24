import { computed, onBeforeUnmount, reactive, ref, watch } from "vue";
import type { ActReport } from "@/composables/useReauth";
import { useServerAction } from "@/composables/useServerAction";
import { reportUiError } from "@/errors/report";
import { answerIssueText } from "@/fiche/messages";
import { t } from "@/i18n";
import {
  type AnalyzeOutcome,
  type FicheDetail,
  failureOf,
  type GameSettingsView,
  getLinkBridge,
  type SaveOutcome,
  type SaveSettingsRequest,
  type ServiceInfo,
} from "@/link";
import {
  type DriftChoices,
  type Edit,
  type Edits,
  editIssues,
  listIssue,
  nothingToSave,
  pendingEdits,
  resettable,
  toSaveRequest,
  wipeEdits,
} from "@/settings/edits";
import { type Filter, isCommon, type Scope } from "@/settings/filter";
import { listIssueText, saveRefusalText, valueText } from "@/settings/messages";
import { modelsOf, proposedOf, type SettingModel } from "@/settings/model";
import { useGameSettingsPendingStore } from "@/stores/gameSettingsPending";
import { useToastsStore } from "@/stores/toasts";

/**
 * L'onglet « Réglages » d'un service de jeu (HRT-109) : la lecture (les valeurs voulues et l'état du fichier de l'agent,
 * le catalogue de la fiche), les changements en cours, les écarts, les réglages inconnus, l'enregistrement et l'import.
 * Tout est INDICATIF : l'agent juge chaque changement, tous ou aucun, et a le dernier mot.
 *
 * Les changements vivent ICI, en mémoire, le temps de l'onglet (une valeur peut être un mot de passe, BR-FICHE-034) : ni
 * Pinia sérialisé, ni stockage local, ni adresse. Ils sont écrasés à l'issue d'un enregistrement et au démontage. Seule
 * la vue « Courants / Tout afficher » est retenue sur le poste (BR-FICHE-045, une préférence d'affichage, pas un réglage
 * du serveur).
 */

export type Phase =
  | "loading"
  | "ready"
  | "absent"
  | "failed"
  | "denied"
  | "no_fiche"
  | "no_catalogue";

/** Le bandeau montré, un seul à la fois, par priorité (design 16.2). */
export type Banner = "imposed" | "unreadable" | "drift" | "pending" | "missing" | "version" | null;

const SCOPE_KEY = "hearth.gameSettings.scope";

function storedScope(): Scope {
  try {
    return window.localStorage.getItem(SCOPE_KEY) === "all" ? "all" : "common";
  } catch {
    return "common";
  }
}

function rememberScope(scope: Scope): void {
  try {
    window.localStorage.setItem(SCOPE_KEY, scope);
  } catch {
    // Pas de stockage (fenêtre privée, données bloquées) : la vue n'est simplement pas retenue.
  }
}

export function useGameSettings(
  serverId: () => string,
  service: () => ServiceInfo,
  connected: () => boolean,
) {
  const toasts = useToastsStore();
  const pendingStore = useGameSettingsPendingStore();
  const action = useServerAction(() => serverId() || null);

  const phase = ref<Phase>("loading");
  const view = ref<GameSettingsView | null>(null);
  const fiche = ref<FicheDetail | null>(null);
  let reads = 0;

  // ── Lire ─────────────────────────────────────────────────────────────────────────────────────
  async function load(): Promise<void> {
    const mine = ++reads;
    if (view.value === null) phase.value = "loading";
    try {
      const bridge = getLinkBridge();
      const read = await bridge.readGameSettings(serverId(), service().key);
      if (mine !== reads) return;
      if (read.kind === "absent") {
        view.value = null;
        phase.value = "absent";
        return;
      }
      const detail = await bridge.readFiche(serverId(), read.view.fiche.id);
      if (mine !== reads) return;
      view.value = read.view;
      if (detail.kind === "unknown") {
        fiche.value = null;
        phase.value = "no_fiche";
      } else {
        fiche.value = detail.fiche;
        phase.value = detail.fiche.settings.length === 0 ? "no_catalogue" : "ready";
      }
    } catch (error) {
      if (mine !== reads) return;
      const failure = failureOf(error);
      if (failure?.kind === "missing_services_right") {
        phase.value = "denied";
        return;
      }
      // Un échec de relecture ne détruit pas ce qui est déjà montré.
      if (view.value === null) phase.value = "failed";
      if (failure === null) reportUiError(error, "gameSettings:read");
    }
  }

  // ── Les changements en cours (mémoire seulement) ────────────────────────────────────────────
  const edits = reactive<Edits>({});
  const driftChoices = reactive<DriftChoices>({});
  const otherValues = reactive<Record<string, string>>({});
  const otherRemoved = reactive(new Set<string>());
  const touched = reactive(new Set<string>());
  const submitted = ref(false);

  function wipe(): void {
    wipeEdits(edits);
    for (const id of Object.keys(driftChoices)) delete driftChoices[id];
    for (const key of Object.keys(otherValues)) otherValues[key] = "";
    for (const key of Object.keys(otherValues)) delete otherValues[key];
    otherRemoved.clear();
    touched.clear();
    submitted.value = false;
  }

  const models = computed<SettingModel[]>(() =>
    fiche.value && view.value ? modelsOf(fiche.value.settings, view.value) : [],
  );
  const labelOf = (id: string): string | null =>
    models.value.find((model) => model.id === id)?.setting.question.label ?? null;

  const filter = ref<Filter>({ query: "", modifiedOnly: false, scope: storedScope() });
  watch(
    () => filter.value.scope,
    (scope) => rememberScope(scope),
  );

  function setValue(id: string, value: string): void {
    edits[id] = { action: "set", value };
  }
  function unset(id: string): void {
    edits[id] = { action: "unset" };
  }
  function reset(id: string): void {
    const model = models.value.find((item) => item.id === id);
    // Un mot de passe n'a pas de valeur proposée : annuler SA saisie, sans toucher à celui qui est défini.
    if (!model || (proposedOf(model) === null && model.setting.question.secret)) {
      delete edits[id];
      return;
    }
    edits[id] = { action: "reset" };
  }
  function touch(id: string): void {
    touched.add(id);
  }

  function setOther(key: string, value: string): void {
    otherValues[key] = value;
  }
  function removeOther(key: string): void {
    otherRemoved.add(key);
  }
  function keepOther(key: string): void {
    otherRemoved.delete(key);
  }
  function chooseDrift(id: string, choice: "keep_file" | "keep_mine"): void {
    driftChoices[id] = choice;
  }

  // ── Ce qui est à enregistrer ─────────────────────────────────────────────────────────────────
  const pending = computed(() => pendingEdits(models.value, edits));
  const otherChanges = computed(() => {
    const out: { key: string; remove: boolean; value: string | null }[] = [];
    for (const other of view.value?.others ?? []) {
      if (otherRemoved.has(other.key)) out.push({ key: other.key, remove: true, value: null });
      else if (
        !other.masked &&
        otherValues[other.key] !== undefined &&
        otherValues[other.key] !== (other.value ?? "")
      ) {
        out.push({ key: other.key, remove: false, value: otherValues[other.key] ?? "" });
      }
    }
    return out;
  });
  const drifts = computed(() => view.value?.drifts ?? []);
  const issues = computed(() => {
    const found = new Map<string, string>();
    for (const [id, issue] of editIssues(models.value, edits)) {
      if (!submitted.value && !touched.has(id)) continue;
      const model = models.value.find((item) => item.id === id);
      if (!model) continue;
      if (issue.code === "list") {
        const edit = edits[id];
        const bad = model && edit?.action === "set" ? listIssue(model.setting, edit.value) : null;
        if (bad) found.set(id, listIssueText(bad, model.setting.list?.itemCharset ?? "unknown"));
      } else found.set(id, answerIssueText(issue));
    }
    return found;
  });
  /** Un défaut de forme existe, montré ou non : l'enregistrement attend qu'il soit corrigé. */
  const invalid = computed(() => editIssues(models.value, edits).size > 0);
  const hasChanges = computed(
    () =>
      !nothingToSave({ changed: pending.value, others: otherChanges.value, drifts: drifts.value }),
  );
  const changeCount = computed(() => pending.value.length + otherChanges.value.length);
  const canSave = computed(
    () => phase.value === "ready" && connected() && hasChanges.value && !invalid.value,
  );
  /** Les réglages qui seront enregistrés, par libellé (la fenêtre de confirmation les liste ; jamais une valeur). */
  const saveLines = computed(() =>
    pending.value.map((model) => {
      const edit = edits[model.id] as Edit;
      const label = model.setting.question.label;
      if (model.setting.question.secret && edit.action === "set") {
        return t("gameSettings.saveListSecret", { label });
      }
      if (edit.action === "unset" || (edit.action === "reset" && proposedOf(model) === null)) {
        return t("gameSettings.saveListRemoved", { label });
      }
      return label;
    }),
  );
  const effect = computed(() => {
    const view_ = view.value;
    if (!view_ || view_.fileStamp === null) return "none" as const;
    return toSaveRequest({
      models: models.value,
      edits,
      drifts: drifts.value,
      choices: driftChoices,
      others: otherChanges.value,
      baseRev: view_.rev,
      fileStamp: view_.fileStamp,
    }).effect;
  });

  // ── Les remises aux valeurs proposées (BR-FICHE-044) ────────────────────────────────────────
  /** Les réglages concernés par une remise en bloc, avec leur valeur actuelle et leur valeur proposée en phrase. */
  function resetItems(themeId: string | null) {
    return models.value
      .filter(
        (model) =>
          (themeId === null || model.setting.theme === themeId) &&
          resettable(model, edits[model.id]),
      )
      .map((model) => {
        const proposed = proposedOf(model);
        const edit = edits[model.id];
        const current =
          edit?.action === "set" ? edit.value : edit?.action === "unset" ? null : model.value;
        return {
          id: model.id,
          label: model.setting.question.label,
          current: valueText(model, current),
          proposed: proposed === null ? null : valueText(model, proposed),
        };
      });
  }
  function applyReset(ids: readonly string[]): void {
    for (const id of ids) edits[id] = { action: "reset" };
  }

  // ── Les bandeaux ─────────────────────────────────────────────────────────────────────────────
  const pendingApply = computed(() => {
    const noted = pendingStore.of(serverId(), service().key);
    if (!noted) return null;
    // Le service a redémarré depuis l'enregistrement : les réglages sont appliqués.
    const current = service();
    if (current.state === "running" && current.since !== null && current.since > noted.at)
      return null;
    return noted;
  });
  const versionKnown = computed(() =>
    models.value.some((model) => model.setting.since !== null || model.setting.until !== null),
  );
  const banner = computed<Banner>(() => {
    const v = view.value;
    if (!v) return null;
    if (v.imposedBroken.length > 0) return "imposed";
    if (v.fileState === "unreadable") return "unreadable";
    if (v.drifts.length > 0 || v.fileChanged) return "drift";
    if (pendingApply.value) return "pending";
    if (v.fileState === "missing") return "missing";
    if (versionKnown.value) return "version";
    return null;
  });
  const imposedBrokenLabels = computed(() =>
    (view.value?.imposedBroken ?? []).map((id) => labelOf(id) ?? id),
  );

  // ── Enregistrer ──────────────────────────────────────────────────────────────────────────────
  /** Dit le résultat à la personne et relit ce que l'agent garde. */
  async function settle(
    outcome: SaveOutcome | null,
    sent: SaveSettingsRequest,
  ): Promise<ActReport> {
    if (outcome === null) return action.failedReport();
    if (outcome.kind === "unknown") {
      toasts.push({ kind: "warn", message: t("operation.unknownNow") });
      toasts.push({ kind: "info", message: t("gameSettings.unknownReread") });
      await load();
      return { kind: "unknown" };
    }
    if (outcome.kind === "refused") {
      const refusal = outcome.refusal;
      // Les réglages ont changé sous les yeux de la personne : on les relit, elle voit ce qui est vrai.
      if (
        refusal.kind === "conflict" ||
        refusal.kind === "file_changed" ||
        refusal.kind === "file_unreadable" ||
        refusal.kind === "service_not_found"
      ) {
        await load();
      }
      if (refusal.kind === "setting_refused") {
        const first = sent.changes.find((change) => change.id === refusal.id);
        if (first) touched.add(first.id);
        submitted.value = true;
      }
      return { kind: "refused", refusal };
    }
    toasts.push({
      kind: "success",
      message: outcome.fileWritten ? t("gameSettings.saved") : t("gameSettings.savedNoChange"),
    });
    if (outcome.adopted.length > 0) {
      toasts.push({
        kind: "info",
        message: t("gameSettings.savedAdopted", { n: outcome.adopted.length }),
      });
    }
    pendingStore.note(serverId(), service().key, outcome.effect, Date.now());
    wipe();
    await load();
    return { kind: "done" };
  }

  /** Enregistre les changements du formulaire. `adminPassword` : `null` sous l'élévation de cinq minutes. */
  async function save(adminPassword: string | null): Promise<ActReport> {
    const current = view.value;
    if (!current || current.fileStamp === null) return { kind: "failed" };
    submitted.value = true;
    const built = toSaveRequest({
      models: models.value,
      edits,
      drifts: drifts.value,
      choices: driftChoices,
      others: otherChanges.value,
      baseRev: current.rev,
      fileStamp: current.fileStamp,
    });
    const outcome = await action.run(() =>
      getLinkBridge().saveGameSettings(serverId(), service().key, built.request, adminPassword),
    );
    return settle(outcome, built.request);
  }

  /** Enregistre l'import d'un texte collé (même acte, origine `import`). */
  async function saveImport(
    request: SaveSettingsRequest,
    adminPassword: string | null,
  ): Promise<ActReport> {
    const outcome = await action.run(() =>
      getLinkBridge().saveGameSettings(serverId(), service().key, request, adminPassword),
    );
    return settle(outcome, request);
  }

  /** Lit un texte collé : l'agent le lit et l'oublie. `null` : la lecture a échoué (déjà notifié). */
  async function analyze(text: string): Promise<AnalyzeOutcome | null> {
    return action.run(() => getLinkBridge().analyzeGameSettings(serverId(), service().key, text));
  }

  /** Les messages d'un refus que la fenêtre de confirmation dit elle-même. */
  const refusalText = (refusal: { kind: string; retry_after_s?: number }) =>
    saveRefusalText(refusal as never, labelOf);

  // ── Le cycle ─────────────────────────────────────────────────────────────────────────────────
  watch(
    [serverId, () => service().key],
    () => {
      wipe();
      view.value = null;
      fiche.value = null;
      phase.value = "loading";
      if (connected()) void load();
    },
    { immediate: true },
  );
  // Reconnexion : on relit (le fichier a pu changer pendant la coupure).
  watch(connected, (up) => {
    if (up) void load();
  });
  onBeforeUnmount(() => {
    wipe();
    reads += 1;
  });

  return {
    phase,
    view,
    fiche,
    models,
    labelOf,
    filter,
    edits,
    driftChoices,
    otherValues,
    otherRemoved,
    setValue,
    unset,
    reset,
    touch,
    setOther,
    removeOther,
    keepOther,
    chooseDrift,
    pending,
    otherChanges,
    drifts,
    issues,
    invalid,
    hasChanges,
    changeCount,
    canSave,
    saveLines,
    effect,
    resetItems,
    applyReset,
    banner,
    pendingApply,
    imposedBrokenLabels,
    save,
    saveImport,
    analyze,
    refusalText,
    reload: load,
    discard: wipe,
    busy: action.busy,
    isCommon,
  };
}

export type GameSettingsState = ReturnType<typeof useGameSettings>;
