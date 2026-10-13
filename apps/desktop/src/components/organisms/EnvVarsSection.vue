<script setup lang="ts">
import { computed, onBeforeUnmount, useId, watch } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HTag from "@/components/atoms/HTag.vue";
import { t } from "@/i18n";
import type { ServiceInfo } from "@/link";
import { canAct, type RightsGate } from "@/services/rights";
import { useServiceEnvStore } from "@/stores/serviceEnv";
import Callout from "../molecules/Callout.vue";
import DashCard from "../molecules/DashCard.vue";
import ServiceRightReason from "../molecules/ServiceRightReason.vue";

// Les variables d'environnement d'un service (HRT-63, BR-SVC-008, BR-SVC-009, BR-SVC-028). Les NOMS viennent de la
// liste ; les VALEURS sont masquées par un masque FIXE de 8 puces (la longueur réelle ne se devine pas) et ne
// viennent qu'après une demande du compte qui a le droit, jamais d'avance. Une valeur n'entre dans aucun titre,
// infobulle, journal, magasin persistant ni le presse-papiers (aucun bouton « copier » ici). Elles sont
// REMASQUÉES, donc effacées de la mémoire, à la fermeture du détail, au changement de service, quand le lien n'est
// plus « Connecté », quand le service disparaît, quand le droit est retiré ou que le moteur n'est plus prêt.
const props = defineProps<{
  serverId: string;
  service: ServiceInfo;
  gate: RightsGate;
  /** L'`id` de la raison écrite (`ServiceRightReason`), liée au bouton désactivé. */
  reasonId: string;
  connected: boolean;
  engineReady: boolean;
  /** Le service n'existe plus : rien ne se révèle. */
  gone: boolean;
}>();
const emit = defineEmits<{ retryRights: [] }>();

const store = useServiceEnvStore();
const noticeId = useId();

const mine = computed(
  () => store.target?.serverId === props.serverId && store.target?.key === props.service.key,
);
const phase = computed(() => (mine.value ? store.phase : "idle"));
const shown = computed(() => phase.value === "shown" && store.variables !== null);
const reading = computed(() => phase.value === "reading");

const reserved = computed(() => props.service.detailsReserved);
const allowed = computed(() => canAct(props.gate));
/** La demande est possible : droit accordé, lien « Connecté », moteur prêt, service présent. */
const possible = computed(
  () => allowed.value && props.connected && props.engineReady && !props.gone,
);

/** Texte permanent qui dit pourquoi le bouton est désactivé quand ce n'est pas le droit (le droit a sa propre raison). */
const engineReason = computed(() =>
  allowed.value && props.connected && !props.engineReady
    ? t("serviceDetail.env.engineNotReady")
    : null,
);
const rightsReason = computed(() => ["denied", "old-agent", "failed"].includes(props.gate.kind));
const describedBy = computed(() => {
  if (rightsReason.value) return props.reasonId;
  if (engineReason.value) return noticeId;
  return undefined;
});

const failure = computed(() => (mine.value && phase.value === "failed" ? store.failure : null));
const failureText = computed(() => {
  if (failure.value === "lost") return t("rights.lost");
  if (failure.value === "unreadable") return t("serviceDetail.env.failed");
  return null;
});

function toggle() {
  if (shown.value) store.clear();
  else void store.reveal(props.serverId, props.service.key);
}

// Remasquage : tout ce qui rend les valeurs douteuses les EFFACE (pas seulement les cache).
function maskIfMine() {
  if (mine.value) store.clear();
}
watch(
  () => [props.serverId, props.service.key],
  () => {
    if (store.target) store.clear();
  },
);
watch(
  () => [props.connected, props.gone, props.engineReady, allowed.value] as const,
  ([connected, gone, ready, ok]) => {
    // Un échec affiché n'est pas une valeur : le message « droit retiré » reste, seules les valeurs sont effacées.
    if ((!connected || gone || !ready || !ok) && store.phase !== "failed") maskIfMine();
  },
);
onBeforeUnmount(maskIfMine);

const rows = computed(() => {
  const revealed = store.variables;
  if (shown.value && revealed) {
    return revealed.map((variable) => ({
      name: variable.name,
      value: variable.value as string | null,
    }));
  }
  return props.service.envNames.map((name) => ({ name, value: null as string | null }));
});
const none = computed(
  () => !reserved.value && rows.value.length === 0 && props.service.envNamesOmitted === 0,
);
/** Une valeur longue ou sur plusieurs lignes défile dans sa zone : elle est alors atteignable au clavier. */
const long = (value: string) => value.length > 160 || value.includes("\n");
</script>

<template>
  <DashCard :title="t('serviceDetail.env.title')" class="env" data-env-section>
    <template v-if="!reserved" #aside>
      <HButton
        variant="secondary"
        size="sm"
        :disabled="!possible && !shown"
        :busy="reading"
        :described-by="describedBy"
        data-env-toggle
        @click="toggle"
      >
        <HIcon :name="shown ? 'eye-off' : 'eye'" size="sm" />
        {{
          reading
            ? t("serviceDetail.env.reading")
            : shown
              ? t("serviceDetail.env.hide")
              : t("serviceDetail.env.reveal")
        }}
      </HButton>
    </template>

    <!-- Service adopté, détail réservé à un compte sans le droit (BR-SVC-028) -->
    <p v-if="reserved" class="env__reserved" data-env-reserved>
      <HIcon name="lock" size="sm" />
      {{ t("serviceDetail.reserved") }}
    </p>

    <template v-else>
      <Callout v-if="rightsReason" inline data-env-reason>
        <ServiceRightReason :gate="gate" :id="reasonId" @retry="emit('retryRights')" />
      </Callout>
      <p v-else-if="engineReason" :id="noticeId" class="env__note" data-env-engine>
        {{ engineReason }}
      </p>

      <Callout v-if="failureText" tone="error" role="alert" inline data-env-failed>
        {{ failureText }}
      </Callout>

      <p class="env__status" role="status" data-env-status>
        <HTag v-if="shown" tone="warn"
          ><HIcon name="eye" size="sm" /> {{ t("serviceDetail.env.visible") }}</HTag
        >
        <span v-else>{{ t("serviceDetail.env.masked") }}</span>
      </p>

      <p v-if="none" class="env__none" data-env-none>{{ t("serviceDetail.env.none") }}</p>
      <ul v-else-if="rows.length > 0" class="env__list" data-env-list>
        <li v-for="(row, index) in rows" :key="index" class="env__row" data-env-row>
          <span class="env__name">{{ row.name }}</span>
          <span v-if="row.value === null" class="env__mask" data-env-mask>
            <span aria-hidden="true">••••••••</span>
            <span class="sr-only">{{ t("serviceDetail.env.maskedValue") }}</span>
          </span>
          <span v-else-if="row.value === ''" class="env__empty">{{
            t("serviceDetail.env.empty")
          }}</span>
          <span
            v-else
            class="env__value"
            :tabindex="long(row.value) ? 0 : undefined"
            data-env-value
            >{{ row.value }}</span
          >
        </li>
      </ul>
      <p v-if="service.envNamesOmitted > 0 && !shown" class="env__note" data-env-omitted>
        {{ t("serviceDetail.env.omitted", { n: service.envNamesOmitted }) }}
      </p>
    </template>
  </DashCard>
</template>

<style scoped>
.env__reserved,
.env__none,
.env__note,
.env__status {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
  color: var(--tx2);
  overflow-wrap: anywhere;
}

.env__list {
  display: flex;
  flex-direction: column;
  margin: 0;
  padding: 0;
  list-style: none;
}

.env__row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr);
  align-items: baseline;
  gap: var(--space-3);
  padding: var(--space-2) 0;
  border-bottom: var(--border-width) solid var(--bd);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
}

.env__row:last-child {
  border-bottom: 0;
}

.env__name {
  font-weight: var(--fw-semibold);
  overflow-wrap: anywhere;
}

.env__mask {
  color: var(--tx2);
}

.env__empty {
  color: var(--tx3);
}

.env__value {
  max-height: var(--env-value-max-height);
  overflow: auto;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  user-select: text;
}
</style>
