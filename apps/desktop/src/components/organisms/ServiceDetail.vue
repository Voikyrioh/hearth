<script setup lang="ts">
import { computed, nextTick, onMounted, ref, useId } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HSkeleton from "@/components/atoms/HSkeleton.vue";
import HTabs from "@/components/atoms/HTabs.vue";
import { formatDateTime } from "@/composables/format";
import { useNow } from "@/composables/useNow";
import { t } from "@/i18n";
import type { ServiceInfo } from "@/link";
import {
  adoptedLine,
  commandText,
  exitCodeText,
  graceText,
  mountView,
  type OpenedService,
  reachText,
  restartText,
} from "@/services/detail";
import { imageLabel, loopBand, portLabel, stateLine } from "@/services/format";
import type { RightsGate } from "@/services/rights";
import Callout from "../molecules/Callout.vue";
import DashCard from "../molecules/DashCard.vue";
import OriginTags from "../molecules/OriginTags.vue";
import ServiceStateBadge from "../molecules/ServiceStateBadge.vue";
import EnvVarsSection from "./EnvVarsSection.vue";

// Le détail d'un service (HRT-63, BR-SVC-007, 016, 023, 027, 028), à droite de la liste. Les données viennent de la
// liste déjà chargée (aucune lecture à l'ouverture) ; seules les valeurs des variables se demandent, à part
// (`EnvVarsSection`). Le détail suit la CLÉ STABLE du service ouvert, jamais son nom : si le service disparaît il
// reste lisible, grisé, avec « Ce service n'existe plus. », et un homonyme créé ensuite est PROPOSÉ, jamais ouvert
// d'office. Tout texte vient de l'agent et est interpolé (jamais du HTML) ; une valeur que ce client ne connaît pas
// s'affiche de façon neutre. Rien n'est inventé pour ce que l'agent ne sait pas encore faire : démarrer, arrêter,
// redémarrer et les journaux sont dits « prochaine version », sans bouton ni onglet actif.
const props = defineProps<{
  serverId: string;
  opened: OpenedService;
  gate: RightsGate;
  reasonId: string;
  connected: boolean;
  engineReady: boolean;
  /** Mettre le focus sur le titre à l'ouverture (ouvert par un geste, jamais par l'adresse). */
  focusTitle: boolean;
}>();
const emit = defineEmits<{ back: []; openOther: [key: string]; retryRights: [] }>();

const now = useNow();
const idBase = useId();
const titleId = `${idBase}-title`;
const logsNoteId = `${idBase}-logs-note`;
const title = ref<HTMLElement | null>(null);
const tab = ref<"detail" | "logs">("detail");

/** Le service montré : l'actuel, ou sa dernière version quand il a disparu (lisible, grisé). */
const service = computed<ServiceInfo | null>(() => {
  const opened = props.opened;
  if (opened.kind === "found") return opened.service;
  if (opened.kind === "gone") return opened.last;
  return null;
});
const gone = computed(() => props.opened.kind === "gone");
const sameName = computed(() => (props.opened.kind === "gone" ? props.opened.sameName : null));

const tabs = computed(() => [
  { id: "detail" as const, label: t("serviceDetail.tabDetail") },
  {
    id: "logs" as const,
    label: t("serviceDetail.tabLogs"),
    disabled: true,
    describedBy: logsNoteId,
  },
]);

onMounted(async () => {
  if (!props.focusTitle) return;
  await nextTick();
  title.value?.focus();
});

const adopted = computed(() =>
  service.value
    ? adoptedLine(service.value, (ms) => formatDateTime(new Date(ms).toISOString()))
    : null,
);
const created = computed(() =>
  service.value?.createdAt != null
    ? formatDateTime(new Date(service.value.createdAt).toISOString())
    : null,
);
const state = computed(() => (service.value ? stateLine(service.value, now.value) : ""));
const loop = computed(() => (service.value ? loopBand(service.value) : ""));
const mounts = computed(() => (service.value ? service.value.mounts.map(mountView) : []));
</script>

<template>
  <section
    class="detail"
    :aria-label="service ? t('serviceDetail.region', { name: service.name }) : t('serviceDetail.unnamed')"
    :aria-busy="opened.kind === 'waiting' ? 'true' : undefined"
    :data-gone="gone ? '' : undefined"
    data-service-detail
    @keydown.esc="emit('back')"
  >
    <header class="detail__head">
      <HButton variant="ghost" size="sm" data-detail-back @click="emit('back')">
        <HIcon name="arrow-left" size="sm" />
        {{ t("serviceDetail.back") }}
      </HButton>

      <!-- Le service disparu : dit en premier, avant tout le reste -->
      <template v-if="gone">
        <Callout tone="info" data-detail-gone>
          {{ t("serviceDetail.gone") }}
        </Callout>
        <Callout v-if="sameName" tone="info" data-detail-recreated>
          {{ t("serviceDetail.recreated", { name: sameName.name }) }}
          <template #actions>
            <HButton variant="secondary" size="sm" data-detail-open-other @click="emit('openOther', sameName.key)">
              {{ t("serviceDetail.openRecreated", { name: sameName.name }) }}
            </HButton>
          </template>
        </Callout>
      </template>

      <div v-if="opened.kind === 'waiting'" class="detail__loading" role="status" :aria-label="t('serviceDetail.loading')" data-detail-loading>
        <HSkeleton size="lg" />
        <HSkeleton size="md" />
        <HSkeleton size="sm" />
      </div>

      <template v-if="service">
        <div class="detail__titleline">
          <h2 :id="titleId" ref="title" class="detail__title" tabindex="-1" data-detail-title>
            {{ service.name }}
          </h2>
          <span class="detail__tags">
            <OriginTags :service="service" part="origin" />
            <OriginTags :service="service" part="details" />
          </span>
        </div>
        <p v-if="!gone" class="detail__state">
          <ServiceStateBadge :state="service.state" />
          <span v-if="state" class="detail__since">{{ state }}</span>
        </p>
        <p v-if="adopted" class="detail__adopted" data-detail-adopted>{{ adopted }}</p>

        <!-- Démarrer, arrêter, redémarrer : pas encore branchés (l'agent ne sait pas encore les faire) -->
        <p v-if="!gone" class="detail__soon" data-detail-actions-soon>
          <HIcon name="info" size="sm" />
          {{ t("serviceDetail.actionsSoon") }}
        </p>

        <HTabs v-model="tab" :tabs="tabs" :label="t('serviceDetail.sections')" :id-base="idBase" />
        <p :id="logsNoteId" class="detail__soon" data-detail-logs-soon>
          <HIcon name="info" size="sm" />
          {{ t("serviceDetail.logsSoon") }}
        </p>
      </template>
    </header>

    <div
      v-if="service"
      :id="`${idBase}-panel-detail`"
      class="detail__body"
      role="tabpanel"
      :aria-labelledby="`${idBase}-tab-detail`"
      :aria-hidden="gone ? 'true' : undefined"
      :inert="gone ? true : undefined"
    >
      <DashCard :title="t('serviceDetail.general')" data-detail-general>
        <dl class="defs">
          <div class="defs__row">
            <dt>{{ t("serviceDetail.state") }}</dt>
            <dd class="defs__state">
              <ServiceStateBadge :state="service.state" />
              <span v-if="state">{{ state }}</span>
            </dd>
          </div>
          <div v-if="service.exitCode !== null" class="defs__row" data-detail-exit>
            <dt>{{ t("serviceDetail.lastStop") }}</dt>
            <dd class="defs__mono">{{ exitCodeText(service.exitCode) }}</dd>
          </div>
          <div v-if="loop" class="defs__row defs__row--wide">
            <dt class="sr-only">{{ t("serviceDetail.recentRestarts") }}</dt>
            <dd>
              <Callout tone="warn" inline data-detail-loop>{{ loop }}</Callout>
            </dd>
          </div>
          <div v-if="created" class="defs__row">
            <dt>{{ t("serviceDetail.created") }}</dt>
            <dd>{{ created }}</dd>
          </div>
          <div v-if="service.graceS !== null" class="defs__row">
            <dt>{{ t("serviceDetail.graceLabel") }}</dt>
            <dd>{{ graceText(service.graceS) }}</dd>
          </div>
          <div v-if="service.origin === 'hearth'" class="defs__row defs__row--wide">
            <dt class="sr-only">{{ t("serviceDetail.resources") }}</dt>
            <dd class="defs__soft">
              <HIcon name="info" size="sm" />
              {{ t("serviceDetail.resources") }}
            </dd>
          </div>
        </dl>
      </DashCard>

      <DashCard :title="t('serviceDetail.imageSection')" data-detail-image>
        <Callout v-if="service.adopted" tone="info" inline data-detail-adopted-note>
          {{ t("serviceDetail.adoptedNote") }}
        </Callout>
        <dl class="defs">
          <div class="defs__row defs__row--stack">
            <dt>{{ t("serviceDetail.image") }}</dt>
            <dd class="defs__mono">{{ imageLabel(service) }}</dd>
          </div>
          <div class="defs__row defs__row--stack">
            <dt>{{ t("serviceDetail.command") }}</dt>
            <dd v-if="service.detailsReserved" class="defs__soft" data-detail-command-reserved>
              <HIcon name="lock" size="sm" />
              {{ t("serviceDetail.reserved") }}
            </dd>
            <dd v-else-if="service.command === null || service.command.length === 0" class="defs__soft">
              {{ t("serviceDetail.commandOfImage") }}
            </dd>
            <dd v-else class="defs__command" data-detail-command>{{ commandText(service.command) }}</dd>
          </div>
        </dl>
      </DashCard>

      <DashCard :title="t('serviceDetail.ports')" data-detail-ports>
        <p v-if="service.hostNetwork" class="defs__soft">{{ t("services.hostNetwork") }}</p>
        <p v-else-if="service.ports.length === 0 && service.portsOmitted === 0" class="defs__soft">
          {{ t("serviceDetail.noPorts") }}
        </p>
        <ul v-else class="rows">
          <li v-for="(port, index) in service.ports" :key="index" class="rows__item">
            <span class="defs__mono">{{ portLabel(port) }}</span>
            <span v-if="reachText(port)" class="defs__soft">{{ reachText(port) }}</span>
          </li>
          <li v-if="service.portsOmitted > 0" class="rows__item defs__soft">
            {{ t("services.portsAlsoOmitted", { n: service.portsOmitted }) }}
          </li>
        </ul>
      </DashCard>

      <DashCard :title="t('serviceDetail.mounts')" data-detail-mounts>
        <p v-if="mounts.length === 0 && service.mountsOmitted === 0" class="defs__soft">
          {{ t("serviceDetail.noMounts") }}
        </p>
        <ul v-else class="rows">
          <li v-for="(mount, index) in mounts" :key="index" class="rows__item rows__item--mount">
            <span class="defs__soft">{{
              mount.kind === "volume" ? t("serviceDetail.volume") : t("serviceDetail.serverFolder")
            }}</span>
            <span class="defs__mono">{{ mount.source }}</span>
            <span class="defs__soft">{{ t("serviceDetail.inService") }}</span>
            <span class="defs__mono">{{ mount.target }}</span>
            <span class="rows__access">{{ mount.access }}</span>
          </li>
          <li v-if="service.mountsOmitted > 0" class="rows__item defs__soft">
            {{ t("services.portsAlsoOmitted", { n: service.mountsOmitted }) }}
          </li>
        </ul>
      </DashCard>

      <EnvVarsSection
        :server-id="serverId"
        :service="service"
        :gate="gate"
        :reason-id="reasonId"
        :connected="connected"
        :engine-ready="engineReady"
        :gone="gone"
        @retry-rights="emit('retryRights')"
      />

      <DashCard :title="t('serviceDetail.restart')" data-detail-restart>
        <p class="detail__sentence">{{ restartText(service.restart) }}</p>
      </DashCard>
    </div>
  </section>
</template>

<style scoped>
.detail {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
  max-width: var(--detail-max);
  min-height: 0;
  overflow-y: auto;
}

.detail[data-gone] .detail__body {
  filter: grayscale(var(--grayscale-stale));
  opacity: var(--opacity-stale);
}

.detail__head {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-2);
  min-width: 0;
}

.detail__head > :deep(.tabs) {
  align-self: stretch;
}

.detail__loading {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  align-self: stretch;
}

.detail__titleline {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2) var(--space-3);
  min-width: 0;
}

.detail__title {
  margin: 0;
  min-width: 0;
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
  overflow-wrap: anywhere;
}

.detail__tags {
  display: inline-flex;
  flex-wrap: wrap;
  gap: var(--space-1);
}

.detail__state {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2) var(--space-3);
  margin: 0;
}

.detail__since,
.detail__adopted {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.detail__adopted {
  margin: 0;
}

.detail__soon {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.detail__body {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  min-width: 0;
}

.detail__sentence {
  margin: 0;
}

.defs {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin: 0;
}

.defs__row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 2fr);
  align-items: baseline;
  gap: var(--space-3);
  min-width: 0;
}

.defs__row--wide,
.defs__row--stack {
  grid-template-columns: minmax(0, 1fr);
  gap: var(--space-1);
}

.defs dt {
  color: var(--tx-secondary);
}

.defs dd {
  margin: 0;
  min-width: 0;
  overflow-wrap: anywhere;
}

.defs__state {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
}

.defs__mono {
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}

.defs__soft {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
  color: var(--tx-secondary);
}

.defs__command {
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-control);
  background: var(--bg);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.rows {
  display: flex;
  flex-direction: column;
  margin: 0;
  padding: 0;
  list-style: none;
}

.rows__item {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: var(--space-1) var(--space-3);
  padding: var(--space-2) 0;
  border-bottom: var(--border-width) solid var(--bd);
  min-width: 0;
}

.rows__item:last-child {
  border-bottom: 0;
}

.rows__item--mount {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: var(--space-half);
}

.rows__access {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}
</style>
