<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HCheckbox from "@/components/atoms/HCheckbox.vue";
import HSkeleton from "@/components/atoms/HSkeleton.vue";
import Callout from "@/components/molecules/Callout.vue";
import ConsoleWaitSection from "@/components/molecules/ConsoleWaitSection.vue";
import EmptyState from "@/components/molecules/EmptyState.vue";
import ImposedBrokenBanner from "@/components/molecules/ImposedBrokenBanner.vue";
import { useFicheLinks } from "@/composables/useFicheLinks";
import { useGameSettings } from "@/composables/useGameSettings";
import type { ActReport } from "@/composables/useReauth";
import { t } from "@/i18n";
import type { ServiceInfo } from "@/link";
import AdminActDialog from "./AdminActDialog.vue";
import FileChangedBanner from "./FileChangedBanner.vue";
import GameSettingsList from "./GameSettingsList.vue";
import OtherSettingsSection from "./OtherSettingsSection.vue";
import RawFileDialog from "./RawFileDialog.vue";
import SettingsImportDialog from "./SettingsImportDialog.vue";
import SettingsResetDialog from "./SettingsResetDialog.vue";

// L'onglet « Réglages » d'un service de jeu (HRT-109, BR-FICHE-037 à 053). Tous les réglages utiles du jeu, rangés par
// thèmes en mots simples : le libellé, l'explication, le défaut du jeu, la valeur proposée, la mention de ce qui se passe
// si on le change. Les libellés, aides, bornes et choix sont ceux de la FICHE (écart 14.26) ; les valeurs voulues et l'état
// du fichier sont ceux de l'AGENT, qui juge chaque changement (tous ou aucun) et a le dernier mot. Aucune règle propre à
// un jeu ici : une fiche de plus ne demande aucun code.
//
// Un seul bandeau à la fois, par priorité : un réglage imposé défait ; un fichier illisible ; un fichier que le jeu a
// réécrit (réglage par réglage, « Garder la valeur du fichier » ou « Remettre la mienne », BR-FICHE-048) ; des
// changements à appliquer ; un fichier disparu ; une version du jeu inconnue. L'enregistrement est un acte confirmé
// (couvert par l'élévation) et ne redémarre rien : la case « Redémarrer maintenant » (décochée) demande le redémarrage
// à la page, qui ouvre sa fenêtre de confirmation habituelle (BR-JEU-035). Les valeurs saisies, mots de passe compris,
// vivent en mémoire seulement et s'effacent à l'issue ou au démontage (BR-FICHE-034).
const props = defineProps<{
  serverId: string;
  service: ServiceInfo;
  /** Le lien est « Connecté » : sinon tout est grisé. */
  connected: boolean;
  /** Le moteur est prêt : sinon on enregistre quand même, mais on ne redémarre pas. */
  engineReady: boolean;
}>();

const emit = defineEmits<{ restart: [service: ServiceInfo] }>();

const state = useGameSettings(
  () => props.serverId,
  () => props.service,
  () => props.connected,
);
const links = useFicheLinks(
  () => props.serverId,
  () => state.fiche.value?.card.id ?? "",
);

const list = ref<InstanceType<typeof GameSettingsList> | null>(null);
const root = ref<HTMLElement | null>(null);

const themes = computed(() => state.fiche.value?.themes ?? []);
const unreadable = computed(() => state.fiche.value?.settingsUnreadable ?? 0);
const canRaw = computed(() => state.view.value?.rawEdit === true);
const saveOpen = ref(false);
const rawOpen = ref(false);
const importOpen = ref(false);
const restartNow = ref(false);
const resetting = ref<{ title: string; themeId: string | null } | null>(null);

const running = computed(() => props.service.state === "running");
const canRestart = computed(() => running.value && props.engineReady);
const afterSave = computed(() => {
  if (!running.value) return t("gameSettings.saveStopped");
  return state.effect.value === "recreate"
    ? t("gameSettings.saveRecreateAfter")
    : t("gameSettings.saveRestartAfter");
});
const importBlocked = computed(() => state.drifts.value.length > 0);
const resetDialogItems = computed(() =>
  resetting.value ? state.resetItems(resetting.value.themeId) : [],
);

watch(saveOpen, (open) => {
  if (open) restartNow.value = false;
});

function openSave() {
  saveOpen.value = true;
}

async function performSave(adminPassword: string | null): Promise<ActReport> {
  const report = await state.save(adminPassword);
  if (report.kind === "done" && restartNow.value && canRestart.value) {
    emit("restart", props.service);
  }
  return report;
}

function openResetTheme(themeId: string) {
  const theme = themes.value.find((item) => item.id === themeId);
  resetting.value = { title: t("gameSettings.resetThemeTitle"), themeId };
  void theme;
}
function openResetAll() {
  resetting.value = { title: t("gameSettings.resetAllTitle"), themeId: null };
}
function confirmReset() {
  state.applyReset(resetDialogItems.value.map((item) => item.id));
  resetting.value = null;
}

// La touche « / » met le curseur dans la recherche, hors d'un champ de saisie. Au RELÂCHEMENT de la touche : la barre
// oblique ne se retrouve ainsi jamais écrite dans le champ qu'on vient de choisir.
function onKeyup(event: KeyboardEvent) {
  if (event.key !== "/" || event.ctrlKey || event.metaKey || event.altKey) return;
  const target = event.target as HTMLElement | null;
  const typing =
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    target?.isContentEditable === true;
  if (typing || state.phase.value !== "ready") return;
  list.value?.focusSearch();
}
onMounted(() => window.addEventListener("keyup", onKeyup));
onBeforeUnmount(() => window.removeEventListener("keyup", onKeyup));

// Le choix par défaut « Garder la valeur du fichier » reçoit le focus quand le bandeau d'écart apparaît.
watch(
  () => state.banner.value,
  async (banner) => {
    if (banner !== "drift") return;
    await nextTick();
    root.value?.querySelector<HTMLElement>("[data-file-changed] button")?.focus();
  },
);

function openToken(id: string) {
  void links.open("token", id);
}
</script>

<template>
  <div ref="root" class="tab" data-settings-tab :aria-busy="state.phase.value === 'loading' ? 'true' : undefined">
    <div
      v-if="state.phase.value === 'loading'"
      class="tab__loading"
      role="status"
      :aria-label="t('gameSettings.loading')"
      data-settings-loading
    >
      <HSkeleton size="lg" />
      <HSkeleton size="md" />
      <HSkeleton size="md" />
      <HSkeleton size="sm" />
    </div>

    <EmptyState
      v-else-if="state.phase.value === 'denied'"
      :title="t('serviceDetail.reserved')"
      text=""
      heading="h2"
      data-settings-denied
    />
    <EmptyState
      v-else-if="state.phase.value === 'absent'"
      :title="t('gameSettings.absentTitle')"
      :text="t('gameSettings.absentText')"
      heading="h2"
      data-settings-absent
    />
    <Callout v-else-if="state.phase.value === 'no_fiche'" tone="warn" data-settings-no-fiche>
      {{ t("gameSettings.ficheMissing") }}
    </Callout>
    <Callout v-else-if="state.phase.value === 'no_catalogue'" tone="warn" data-settings-no-catalogue>
      {{ t("gameSettings.noCatalogue") }}
    </Callout>
    <Callout v-else-if="state.phase.value === 'failed'" tone="error" role="alert" data-settings-failed>
      {{ t("gameSettings.readFailed") }}
      <template #actions>
        <HButton variant="secondary" size="sm" data-settings-retry @click="state.reload()">
          {{ t("common.retry") }}
        </HButton>
      </template>
    </Callout>

    <template v-else-if="state.phase.value === 'ready' && state.view.value">
      <!-- Un seul bandeau à la fois, par priorité (design 16.2) -->
      <ImposedBrokenBanner
        v-if="state.banner.value === 'imposed'"
        :labels="state.imposedBrokenLabels.value"
      />
      <Callout
        v-else-if="state.banner.value === 'unreadable'"
        tone="warn"
        role="alert"
        data-settings-unreadable-file
      >
        {{ t("gameSettings.fileUnreadable") }}
      </Callout>
      <FileChangedBanner
        v-else-if="state.banner.value === 'drift'"
        :drifts="state.drifts.value"
        :models="state.models.value"
        :choices="state.driftChoices"
        :disabled="!connected"
        @choose="state.chooseDrift"
      />
      <Callout v-else-if="state.banner.value === 'pending'" tone="info" data-settings-pending>
        {{
          state.pendingApply.value?.effect === "recreate"
            ? t("gameSettings.pendingRecreate")
            : t("gameSettings.pendingRestart")
        }}
        <template v-if="canRestart" #actions>
          <HButton
            variant="secondary"
            size="sm"
            :disabled="!connected"
            data-settings-restart
            @click="emit('restart', service)"
          >
            {{ t("gameSettings.pendingRestartNow") }}
          </HButton>
        </template>
      </Callout>
      <Callout v-else-if="state.banner.value === 'missing'" tone="info" data-settings-missing>
        {{ t("gameSettings.fileMissing") }}
      </Callout>
      <Callout v-else-if="state.banner.value === 'version'" tone="info" data-settings-version>
        {{ t("gameSettings.versionUnknown") }}
      </Callout>

      <GameSettingsList
        ref="list"
        v-model:filter="state.filter.value"
        :models="state.models.value"
        :themes="themes"
        :edits="state.edits"
        mode="service"
        :issues="state.issues.value"
        :disabled="!connected"
        @set="state.setValue"
        @unset="state.unset"
        @reset="state.reset"
        @reset-theme="openResetTheme"
        @blur="state.touch"
        @link="openToken"
      >
        <template #after>
          <Callout v-if="unreadable > 0" tone="info" inline data-settings-unreadable>
            {{ t("gameSettings.unreadable", { n: unreadable }) }}
          </Callout>
          <OtherSettingsSection
            :others="state.view.value.others"
            :values="state.otherValues"
            :removed="state.otherRemoved"
            :disabled="!connected"
            @set="state.setOther"
            @remove="state.removeOther"
            @keep="state.keepOther"
          />
          <ConsoleWaitSection />
        </template>
      </GameSettingsList>

      <!-- Le pied collant : enregistrer, annuler, et les gestes de masse -->
      <footer class="tab__foot" data-settings-footer>
        <div class="tab__tools">
          <HButton
            variant="secondary"
            size="sm"
            :disabled="!connected || importBlocked"
            :hint="importBlocked ? t('gameSettings.importBlocked') : undefined"
            data-settings-import
            @click="importOpen = true"
          >
            {{ t("gameSettings.import") }}
          </HButton>
          <HButton
            variant="secondary"
            size="sm"
            :disabled="!connected"
            data-settings-reset-all
            @click="openResetAll"
          >
            {{ t("gameSettings.resetAll") }}
          </HButton>
          <HButton
            v-if="canRaw"
            variant="secondary"
            size="sm"
            :disabled="!connected"
            data-settings-raw
            @click="rawOpen = true"
          >
            {{ t("gameSettings.raw") }}
          </HButton>
        </div>
        <p v-if="!canRaw" class="tab__no-raw" data-settings-no-raw>{{ t("gameSettings.noRaw") }}</p>
        <div class="tab__save">
          <p class="tab__count" role="status" data-settings-count>
            {{
              state.invalid.value
                ? t("gameSettings.footerFixIssues")
                : state.hasChanges.value
                  ? t("gameSettings.footerChanges", { n: state.changeCount.value })
                  : t("gameSettings.footerClean")
            }}
          </p>
          <HButton
            variant="ghost"
            :disabled="!state.hasChanges.value && state.drifts.value.length === 0"
            data-settings-discard
            @click="state.discard()"
          >
            {{ t("gameSettings.discard") }}
          </HButton>
          <HButton
            :disabled="!state.canSave.value"
            :busy="state.busy.value"
            data-settings-save
            @click="openSave"
          >
            {{ t("gameSettings.save") }}
          </HButton>
        </div>
      </footer>

      <AdminActDialog
        :open="saveOpen"
        :server-id="serverId"
        kind="game_settings_save"
        :title="t('gameSettings.saveTitle')"
        :submit-label="t('gameSettings.save')"
        :can-submit="state.canSave.value"
        :perform="performSave"
        :refusal-text="state.refusalText"
        data-save-dialog
        @close="saveOpen = false"
      >
        <p class="tab__summary" data-save-summary>
          {{ t("gameSettings.saveSummary", { n: state.pending.value.length }) }}
        </p>
        <ul v-if="state.saveLines.value.length > 0" class="tab__lines" data-save-lines>
          <li v-for="line in state.saveLines.value" :key="line">{{ line }}</li>
        </ul>
        <p v-if="state.otherChanges.value.length > 0" class="tab__summary">
          {{ t("gameSettings.saveOthers", { n: state.otherChanges.value.length }) }}
        </p>
        <HCheckbox
          v-if="running"
          v-model="restartNow"
          :label="t('gameSettings.saveRestartNow')"
          :disabled="!canRestart"
          data-save-restart
        />
        <p class="tab__after" data-save-after>
          {{ afterSave }}
        </p>
        <p v-if="running && !engineReady" class="tab__after">{{ t("gameSettings.saveNoEngine") }}</p>
      </AdminActDialog>

      <SettingsResetDialog
        :open="resetting !== null"
        :title="resetting?.title ?? ''"
        :items="resetDialogItems"
        @confirm="confirmReset"
        @cancel="resetting = null"
      />

      <RawFileDialog :open="rawOpen" @close="rawOpen = false" />

      <SettingsImportDialog
        :open="importOpen"
        :server-id="serverId"
        :models="state.models.value"
        :base-rev="state.view.value.rev"
        :file-stamp="state.view.value.fileStamp ?? ''"
        :analyze="state.analyze"
        :save="state.saveImport"
        :label-of="state.labelOf"
        @close="importOpen = false"
      />
    </template>
  </div>
</template>

<style scoped>
.tab {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
  max-width: var(--detail-max);
}

.tab__loading {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.tab__foot {
  position: sticky;
  bottom: 0;
  z-index: var(--z-raised);
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3) var(--space-4);
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-card);
  background: var(--card-2);
  box-shadow: var(--card-edge);
}

.tab__tools,
.tab__save {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2) var(--space-3);
}

.tab__count {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.tab__no-raw {
  flex: 1 1 100%;
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.tab__summary,
.tab__after {
  margin: 0;
}

.tab__after {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.tab__lines {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  margin: 0;
  padding-left: var(--space-5);
}
</style>
