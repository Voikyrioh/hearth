<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HMeter from "@/components/atoms/HMeter.vue";
import { t } from "@/i18n";
import type { Transfer } from "@/link";
import {
  canCancel,
  canDismiss,
  canResume,
  needsAction,
  progressText,
  ratioOf,
  statusText,
  waitsForRoom,
} from "@/transfers/messages";

// Le panneau « Transferts » (HRT-96, BR-FILES-019, BR-MOD-015) : le lieu où retrouver un envoi de fichier
// quand on a quitté l'écran qui l'a lancé. Visible de tous les écrans d'un serveur ; absent quand il n'y a rien.
// Une ligne par envoi : le nom du fichier, une jauge en octets, l'état, puis les gestes qui ont un sens : « Reprendre
// l'envoi » (jamais automatique) d'un envoi interrompu, « Annuler » d'un envoi ouvert (la partie reçue est retirée du
// serveur), « Retirer de la liste » d'un envoi fini. Aucun contenu de fichier ici : des nombres et le nom choisi.
const props = defineProps<{ transfers: readonly Transfer[] }>();
const emit = defineEmits<{
  resume: [transfer: Transfer];
  cancel: [transfer: Transfer];
  dismiss: [transfer: Transfer];
}>();

const VISIBLE = 3;
const expanded = ref(false);
// Les envois qui demandent un geste passent d'abord : ils ne se cachent jamais sous « +2 autres » (Nora, 16/11).
const ordered = computed(() => [
  ...props.transfers.filter((item) => needsAction(item)),
  ...props.transfers.filter((item) => !needsAction(item)),
]);
const shown = computed(() => (expanded.value ? ordered.value : ordered.value.slice(0, VISIBLE)));
const hidden = computed(() => Math.max(0, props.transfers.length - VISIBLE));
watch(hidden, (n) => {
  if (n === 0) expanded.value = false;
});

// L'issue d'un envoi est annoncée à l'assistance vocale quand elle change (un envoi qui finit, qui se coupe), poliment
// et après 500 ms ; la progression chiffrée n'est jamais annoncée.
const announced = ref("");
let timer: ReturnType<typeof setTimeout> | undefined;
const seen = new Map<string, string>();
watch(
  () => props.transfers.map((item) => `${item.id}:${item.status}`),
  () => {
    let message = "";
    for (const item of props.transfers) {
      const before = seen.get(item.id);
      seen.set(item.id, item.status);
      if (before === undefined || before === item.status) continue;
      if (item.status === "done") message = t("transfers.announceDone", { name: item.name });
      if (item.status === "interrupted") {
        message = waitsForRoom(item)
          ? t("transfers.announceNoSpace", { name: item.name })
          : t("transfers.announceInterrupted", { name: item.name });
      }
    }
    if (message === "") return;
    clearTimeout(timer);
    timer = setTimeout(() => {
      announced.value = message;
    }, 500);
  },
  { immediate: true },
);
onBeforeUnmount(() => clearTimeout(timer));
</script>

<template>
  <section
    v-if="transfers.length > 0"
    class="strip"
    role="region"
    :aria-label="t('transfers.stripRegion')"
    data-transfers-strip
  >
    <h2 class="strip__title">{{ t("transfers.stripTitle") }}</h2>
    <p class="sr-only" role="status" aria-live="polite">{{ announced }}</p>
    <ul class="strip__list">
      <li
        v-for="transfer in shown"
        :key="transfer.id"
        class="row"
        :data-transfer="transfer.id"
        :data-status="transfer.status"
      >
        <div class="row__text">
          <span class="row__title" data-transfer-name>{{ transfer.name }}</span>
          <span class="row__status" data-transfer-status>{{ statusText(transfer) }}</span>
        </div>
        <div v-if="transfer.status !== 'cancelled'" class="row__meter">
          <HMeter
            :ratio="ratioOf(transfer)"
            :label="t('transfers.progressLabel', { name: transfer.name })"
          />
        </div>
        <span v-if="transfer.status !== 'cancelled'" class="row__figures" data-transfer-progress>{{
          progressText(transfer)
        }}</span>
        <div class="row__acts">
          <HButton
            v-if="canResume(transfer)"
            :variant="waitsForRoom(transfer) ? 'secondary' : 'primary'"
            size="sm"
            :aria-label="
              waitsForRoom(transfer)
                ? t('transfers.actions.retryLabel', { name: transfer.name })
                : t('transfers.actions.resumeLabel', { name: transfer.name })
            "
            data-transfer-resume
            @click="emit('resume', transfer)"
          >
            {{ waitsForRoom(transfer) ? t("transfers.actions.retry") : t("transfers.actions.resume") }}
          </HButton>
          <HButton
            v-if="canCancel(transfer)"
            variant="secondary"
            size="sm"
            :aria-label="t('transfers.actions.cancelLabel', { name: transfer.name })"
            data-transfer-cancel
            @click="emit('cancel', transfer)"
          >
            {{ t("transfers.actions.cancel") }}
          </HButton>
          <HButton
            v-if="canDismiss(transfer)"
            variant="ghost"
            size="sm"
            :aria-label="t('transfers.actions.dismissLabel', { name: transfer.name })"
            data-transfer-dismiss
            @click="emit('dismiss', transfer)"
          >
            {{ t("transfers.actions.dismiss") }}
          </HButton>
        </div>
      </li>
    </ul>
    <HButton
      v-if="hidden > 0"
      variant="ghost"
      size="sm"
      :aria-expanded="expanded"
      data-transfers-more
      @click="expanded = !expanded"
    >
      {{ expanded ? t("serviceOps.less") : t("serviceOps.more", { n: hidden }) }}
    </HButton>
  </section>
</template>

<style scoped>
.strip {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.strip__title {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  font-weight: var(--fw-semibold);
  letter-spacing: var(--ls-label);
  text-transform: uppercase;
}

.strip__list {
  max-height: var(--strip-list-max);
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.row {
  display: grid;
  grid-template-columns: var(--op-columns);
  align-items: center;
  gap: var(--space-3);
  min-height: var(--target-size);
  min-width: 0;
}

/* Sous 960 px la jauge passe sous le libellé. */
@media (max-width: 960px) {
  .row {
    grid-template-columns: minmax(0, 1fr) auto;
  }

  .row__meter,
  .row__figures {
    grid-column: 1 / -1;
  }
}

.row__text {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  min-width: 0;
}

.row__title {
  color: var(--tx);
  font-size: var(--fs-body);
  font-weight: var(--fw-medium);
  overflow-wrap: anywhere;
}

.row__status,
.row__figures {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.row__figures {
  font-family: var(--font-mono);
  white-space: nowrap;
}

.row__acts {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: var(--space-2);
}
</style>
