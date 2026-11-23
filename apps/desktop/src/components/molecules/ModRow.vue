<script setup lang="ts">
import { computed, useId } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HTag from "@/components/atoms/HTag.vue";
import HToggle from "@/components/atoms/HToggle.vue";
import { t } from "@/i18n";
import type { ModItem, ModsGame, ModsView } from "@/link";
import {
  fileLabel,
  type Gesture,
  gestureGate,
  loaderName,
  modName,
  originLabel,
  pendingText,
  statePill,
  verdictSentence,
} from "@/services/mods";
import type { RightsGate } from "@/services/rights";

// Une ligne de la liste des mods (HRT-261, HRT-262, maquette Main) : nom et fichier, version, moteur, version de
// Minecraft visée, état (le verdict en une pastille et, sous le nom, une phrase), origine, interrupteur « Activé »
// (`role="switch"`, cible de 44 px) et « Retirer » / « Remettre » / « Annuler ». Un geste préparé dit ce qu'il fera au
// prochain redémarrage (interrupteur en pointillé, « Annuler » à la place de « Retirer »). Aucun geste ne part d'ici :
// la ligne émet, le lot est tenu par `useServiceMods`. Un mod du pack Modrinth et un fichier au nom illisible ont
// l'interrupteur et « Retirer » inactifs avec leur raison ; sans le droit, la raison est celle que le détail écrit déjà.
const props = defineProps<{
  item: ModItem;
  game: ModsGame;
  /** Le verdict mentionne d'autres mods par leur nom : la liste entière sert à les retrouver. */
  view: ModsView;
  gate: RightsGate;
  /** L'`id` de la raison du droit que le détail écrit déjà, une seule fois (BR-GRANT-014). */
  rightsReasonId: string;
  /** `add` : un fichier déposé qui n'est pas encore sur le serveur ; `replaced` : l'ancien fichier d'un remplacement. */
  pending?: Gesture | "add" | "replaced";
  /** La phrase du geste préparé quand elle porte un nom ou une version (ajout, remplacement). */
  pendingLabel?: string;
  /** Une phrase posée sous le nom : « Le mod X a besoin de celui-ci », ou le refus de réactiver. */
  note?: string | null;
  /** Le lien est coupé : aucun geste ne se prépare. */
  linkDown?: boolean;
}>();
const emit = defineEmits<{
  toggle: [];
  retire: [];
  restore: [];
  undo: [];
}>();

const reasonId = useId();
const name = computed(() => modName(props.item));
const file = computed(() => fileLabel(props.item));
const retired = computed(() => props.item.place === "retired");
const pill = computed(() => statePill(props.item));
const sentence = computed(() => verdictSentence(props.item, props.game, props.view));
const gesture = computed(() => gestureGate(props.item, props.gate));
const blocked = computed(() => !gesture.value.allowed || props.linkDown === true || fixed.value);
const reason = computed(() => gesture.value.reason);
const describedBy = computed(() => {
  if (gesture.value.rights) return props.rightsReasonId;
  return reason.value ? reasonId : undefined;
});
/** L'interrupteur montre l'état APRÈS le geste préparé. */
const switchOn = computed(() => {
  if (props.pending === "disable") return false;
  if (props.pending === "enable") return true;
  return props.item.enabled;
});
const switchPending = computed(() => props.pending === "disable" || props.pending === "enable");
const fixed = computed(() => props.pending === "add" || props.pending === "replaced");
const targetMinecraft = computed(() => {
  switch (props.item.verdict.code) {
    case "compatible":
      return props.game.minecraft ?? t("mods.targetUnknown");
    case "incompatible_minecraft":
      return t("mods.targetOther");
    default:
      return t("mods.targetUnknown");
  }
});
</script>

<template>
  <tr
    :class="['row', { 'row--off': retired || !item.enabled, 'row--pending': pending !== undefined }]"
    :data-mod-row="file ?? `rang-${item.rank}`"
    :data-mod-pending="pending"
  >
    <td>
      <div class="row__name">
        <strong class="row__title">{{ name }}</strong>
        <span v-if="file" class="row__file">{{ file }}</span>
        <span v-if="pending" class="row__pending" data-mod-pending-text>{{ pendingLabel ?? (pending === 'add' || pending === 'replaced' ? '' : pendingText(pending)) }}</span>
        <span v-if="sentence && !retired" class="row__sentence" data-mod-sentence>{{ sentence }}</span>
        <span v-if="note" class="row__note" data-mod-note>{{ note }}</span>
      </div>
    </td>
    <td class="row__mono">{{ item.identity?.version ?? "" }}</td>
    <td>{{ item.identity ? loaderName(item.identity.loader) : "" }}</td>
    <td class="row__mono">{{ item.identity ? targetMinecraft : "" }}</td>
    <td>
      <HTag :tone="pill.tone === 'neutral' ? 'neutral' : pill.tone" data-mod-state>
        <HIcon
          :name="pill.tone === 'ok' ? 'check' : pill.tone === 'crit' ? 'close' : pill.tone === 'warn' ? 'alert' : 'info'"
          size="sm"
        />
        {{ pill.label }}
      </HTag>
    </td>
    <td><span class="row__origin" data-mod-origin>{{ originLabel(item.origin) }}</span></td>
    <td class="row__switch">
      <HToggle
        v-if="!retired && pending !== 'add'"
        :model-value="switchOn"
        :disabled="blocked"
        :hint="reason ?? undefined"
        :aria-label="t('mods.switchLabel', { name })"
        :described-by="describedBy"
        :class="{ 'row__toggle--pending': switchPending }"
        data-mod-switch
        @update:model-value="emit('toggle')"
      />
    </td>
    <td class="row__action">
      <HButton v-if="pending" variant="secondary" size="sm" data-mod-undo @click="emit('undo')">
        {{ t("mods.undo") }}
      </HButton>
      <HButton
        v-else-if="retired"
        variant="secondary"
        size="sm"
        :disabled="blocked"
        :hint="reason ?? undefined"
        :described-by="describedBy"
        data-mod-restore
        @click="emit('restore')"
      >
        {{ t("mods.restore") }}
      </HButton>
      <HButton
        v-else
        variant="secondary"
        size="sm"
        :disabled="blocked"
        :hint="reason ?? undefined"
        :described-by="describedBy"
        data-mod-retire
        @click="emit('retire')"
      >
        {{ t("mods.retire") }}
      </HButton>
      <span v-if="reason" :id="reasonId" class="row__reason" data-mod-reason>{{ reason }}</span>
    </td>
  </tr>
</template>

<style scoped>
.row td {
  padding: var(--space-2) var(--space-3);
  border-top: var(--border-width) solid var(--bd);
  vertical-align: middle;
}

.row--off .row__title,
.row--off .row__mono {
  color: var(--tx-faint);
}

.row--pending {
  background: var(--card-2);
}

.row__name {
  display: flex;
  flex-direction: column;
  gap: var(--space-half);
  min-width: 0;
}

.row__title {
  overflow-wrap: anywhere;
  font-weight: var(--fw-semibold);
}

.row__file {
  color: var(--tx-faint);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}

.row__pending {
  color: var(--cool);
  font-size: var(--fs-small);
}

.row__sentence,
.row__note {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.row__note {
  color: var(--warn);
}

.row__mono {
  font-family: var(--font-mono);
  font-size: var(--fs-small);
}

.row__origin {
  display: inline-block;
  padding: var(--space-half) var(--space-2);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-pill);
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  white-space: nowrap;
}

.row__switch {
  text-align: center;
}

.row__toggle--pending {
  outline: var(--border-width) dashed var(--cool);
  outline-offset: var(--space-half);
}

.row__action {
  text-align: right;
  white-space: nowrap;
}

.row__reason {
  display: block;
  margin-top: var(--space-1);
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  white-space: normal;
}
</style>
