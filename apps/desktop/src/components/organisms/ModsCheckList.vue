<script setup lang="ts">
import { computed, ref } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import HCheckbox from "@/components/atoms/HCheckbox.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HTag from "@/components/atoms/HTag.vue";
import type { PendingAdd } from "@/composables/useServiceMods";
import { t } from "@/i18n";
import type { ModCheckFile, ModsGame } from "@/link";
import { loaderName, modName } from "@/services/mods";
import { acceptedSentence, checkTitle, refusalSentence } from "@/services/modsDeposit";

// Le contrôle des fichiers déposés (HRT-262, BR-MOD-021 à 023, maquette Controle) : un verdict par fichier, dit en une phrase.
// Un fichier accepté s'ajoute ; un fichier qui remplace un mod installé a sa case « Remplacer… », DÉCOCHÉE d'office (non
// cochée, le fichier est laissé de côté, jamais un remplacement en silence) ; un fichier refusé dit seulement qu'il n'est
// pas compatible ou pourquoi, jamais une plage de versions, et quitte la liste. « Continuer » met les ajouts dans le lot ;
// rien n'est écrit sur le serveur avant « Appliquer ». Un mod qui en demande un autre propose de déposer d'autres fichiers.
const props = defineProps<{
  files: readonly ModCheckFile[];
  game: ModsGame;
  /** Le nom sur le poste d'un fichier déposé (identifiant du dépôt -> nom). */
  nameOf: (deposit: string) => string;
  /** L'empreinte du fichier local, celle que l'agent compare à la sienne. */
  shaOf: (deposit: string) => string | null;
  /** Combien d'ajouts le lot peut encore recevoir (8 au plus). */
  room: number;
}>();
const emit = defineEmits<{
  continue: [entries: PendingAdd[]];
  more: [];
  drop: [deposit: string];
}>();

const replace = ref(new Map<string, boolean>());
const isAccepted = (file: ModCheckFile) => file.outcome === "add" || file.outcome === "replace";

function setReplace(deposit: string, on: boolean) {
  replace.value = new Map(replace.value).set(deposit, on);
}

/** Les fichiers qui entrent dans le lot : les ajouts, et les remplacements dont la case est cochée. */
const chosen = computed(() =>
  props.files.filter(
    (file) =>
      file.outcome === "add" || (file.outcome === "replace" && replace.value.get(file.deposit)),
  ),
);
const overRoom = computed(() => chosen.value.length > props.room);

function toneOf(file: ModCheckFile): "ok" | "warn" | "crit" {
  if (!isAccepted(file)) return "crit";
  return file.verdict?.code === "compatible" ? "ok" : "warn";
}

function pillOf(file: ModCheckFile): string {
  if (!isAccepted(file)) return t("mods.verdict.incompatible");
  switch (file.verdict?.code) {
    case "to_complete":
      return t("mods.verdict.toComplete");
    case "unverified":
      return t("mods.verdict.unverified");
    default:
      return t("mods.verdict.compatible");
  }
}

function titleOf(file: ModCheckFile): string {
  if (file.identity) {
    const name = modName({ identity: file.identity, file: null, rank: 0, readableName: false });
    return file.identity.version ? `${name} ${safeText(file.identity.version)}` : name;
  }
  return props.nameOf(file.deposit);
}

function done() {
  const entries: PendingAdd[] = [];
  for (const file of chosen.value) {
    const sha256 = props.shaOf(file.deposit);
    if (sha256 === null || file.targetFile === null) continue;
    entries.push({
      deposit: file.deposit,
      sha256,
      identity: file.identity,
      verdict: file.verdict,
      targetFile: file.targetFile,
      replace: file.outcome === "replace" ? (file.replaces?.file ?? null) : null,
      replaceVersion: file.replaces?.version ?? null,
    });
  }
  emit("continue", entries);
}
</script>

<template>
  <section class="mc" data-mods-check>
    <h3 class="mc__title" data-mods-check-title>{{ checkTitle(files) }}</h3>
    <p class="mc__lead">{{ t("mods.check.lead", { loader: loaderName(game.loader), minecraft: game.minecraft ?? "?" }) }}</p>

    <ul class="mc__list">
      <li v-for="file in files" :key="file.deposit" class="mc__file" data-mods-check-file :data-outcome="file.outcome">
        <div class="mc__head">
          <strong class="mc__name">{{ titleOf(file) }}</strong>
          <HTag :tone="toneOf(file)" data-mods-check-pill>
            <HIcon :name="toneOf(file) === 'ok' ? 'check' : toneOf(file) === 'warn' ? 'alert' : 'close'" size="sm" />
            {{ pillOf(file) }}
          </HTag>
          <span class="mc__file-name">{{ nameOf(file.deposit) }}</span>
        </div>
        <p class="mc__sentence" data-mods-check-sentence>
          {{ isAccepted(file) ? acceptedSentence(file, game) : refusalSentence(file, game) }}
        </p>
        <HCheckbox
          v-if="file.outcome === 'replace' && file.replaces"
          :model-value="replace.get(file.deposit) ?? false"
          :label="
            t('mods.check.replace', {
              old: safeText(file.replaces.file),
              oldVersion: file.replaces.version ? safeText(file.replaces.version) : '',
              newVersion: file.identity?.version ? safeText(file.identity.version) : '',
            })
          "
          data-mods-check-replace
          @update:model-value="setReplace(file.deposit, $event)"
        />
        <div v-if="isAccepted(file) && file.verdict?.code === 'to_complete'" class="mc__row">
          <HButton variant="secondary" size="sm" data-mods-check-more @click="emit('more')">
            {{ t("mods.check.depositMissing") }}
          </HButton>
        </div>
        <div v-if="!isAccepted(file)" class="mc__row">
          <HButton variant="secondary" size="sm" data-mods-check-drop @click="emit('drop', file.deposit)">
            {{ t("mods.check.drop") }}
          </HButton>
        </div>
      </li>
    </ul>

    <p v-if="overRoom" class="mc__room" role="alert" data-mods-check-room>
      {{ t("mods.check.room", { n: room }) }}
    </p>
    <div class="mc__actions">
      <HButton variant="primary" :disabled="chosen.length === 0 || overRoom" data-mods-check-continue @click="done()">
        {{ t(chosen.length === 1 ? "mods.check.continueOne" : "mods.check.continue", { n: chosen.length }) }}
      </HButton>
      <HButton variant="secondary" data-mods-check-back @click="emit('more')">
        {{ t("mods.check.more") }}
      </HButton>
    </div>
    <p class="mc__warning">{{ t("mods.warning") }}</p>
  </section>
</template>

<style scoped>
.mc {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

.mc__title {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.mc__lead,
.mc__warning,
.mc__sentence {
  margin: 0;
  color: var(--tx-secondary);
}

.mc__warning {
  font-size: var(--fs-small);
}

.mc__list {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  margin: 0;
  padding: 0;
  list-style: none;
}

.mc__file {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.mc__head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
}

.mc__head :deep(.tag) {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
}

.mc__name {
  font-weight: var(--fw-semibold);
}

.mc__file-name {
  color: var(--tx-faint);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}

.mc__row,
.mc__actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
}

.mc__room {
  margin: 0;
  color: var(--warn);
}
</style>
