<script setup lang="ts">
import { computed, ref, watch } from "vue";
import HCheckbox from "@/components/atoms/HCheckbox.vue";
import HTag from "@/components/atoms/HTag.vue";
import Callout from "@/components/molecules/Callout.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import type { ActReport } from "@/composables/useReauth";
import type { Draft } from "@/deploy/draft";
import { shownRepository, shownVersion } from "@/deploy/form";
import { portVerdict, refusalText } from "@/deploy/messages";
import { t } from "@/i18n";
import type { DeployCheck } from "@/link";
import { formatCpu, formatMemory } from "@/resources";
import { formatSize } from "@/serviceOps/messages";

// Le récapitulatif d'un déploiement (HRT-69, BR-DEPLOY-015, 018, 031) : UNE fenêtre pour tout ce qui va partir ET la
// confirmation du socle (`AdminActDialog`, acte `service_deploy`, JAMAIS couvert par l'élévation de 5 minutes : le
// mot de passe est demandé même juste après un autre acte). Elle montre le nom, l'image et la version, les ports (et
// qui peut les joindre), le nombre de variables (leurs valeurs masquées, jamais en clair), la commande oui ou non, le
// redémarrage, les ressources, puis les POINTS D'ATTENTION qui s'appliquent : chacun une case, et « Déployer le
// service » reste inactif tant qu'un point n'est pas validé. Les points viennent de la consultation faite à
// l'ouverture (`check`) ; le serveur les refait au lancement et a le dernier mot (BR-DEPLOY-032). « Modifier » ferme et
// revient au formulaire intact. La commande n'est pas répétée ici : la fenêtre dit seulement si elle est personnalisée.
const props = defineProps<{
  open: boolean;
  serverId: string;
  draft: Draft;
  /** Les constats frais du serveur à l'ouverture ; `null` : la consultation n'a pas pu être faite. */
  check: DeployCheck | null;
  /** Le serveur vient de refuser faute de place (`freeBytes` : `null` s'il ne dit pas combien) : la case est exigée même si la consultation ne l'avait pas vu. */
  forceLowDisk: { freeBytes: number | null } | null;
  perform: (
    adminPassword: string | null,
    accept: { reservedPorts: boolean; lowDisk: boolean },
  ) => Promise<ActReport>;
}>();
const emit = defineEmits<{ close: [] }>();

const reservedOk = ref(false);
const diskOk = ref(false);
watch(
  () => props.open,
  () => {
    reservedOk.value = false;
    diskOk.value = false;
  },
);

const version = computed(() => shownVersion(props.draft));
const repository = computed(() => shownRepository(props.draft));

/** Les ports du formulaire que le serveur dit réservés par un service arrêté (BR-DEPLOY-008). */
const reservedPorts = computed(() => {
  const answer = props.check;
  if (!answer) return [];
  return answer.ports.filter((port) => port.status === "reserved_by_stopped");
});
const lowDisk = computed<{ freeBytes: number | null } | null>(() => {
  if (props.forceLowDisk !== null) return props.forceLowDisk;
  const disk = props.check?.disk;
  return disk?.low && !props.check?.imagePresent ? { freeBytes: disk.freeBytes } : null;
});
const lowDiskText = computed(() =>
  lowDisk.value && lowDisk.value.freeBytes !== null
    ? t("deploy.lowDisk", { space: formatSize(lowDisk.value.freeBytes) })
    : t("deploy.lowDiskUnknown"),
);
const imagePresent = computed(() => props.check?.imagePresent === true);

const needsAttention = computed(() => reservedPorts.value.length > 0 || lowDisk.value !== null);
const attentionDone = computed(
  () =>
    (reservedPorts.value.length === 0 || reservedOk.value) &&
    (lowDisk.value === null || diskOk.value),
);

const envCount = computed(() => props.draft.env.length);
const protocolText = (protocol: string) => protocol.toUpperCase();

const memory = computed(() =>
  props.draft.resources?.memoryBytes != null ? formatMemory(props.draft.resources.memoryBytes) : "",
);
const cpu = computed(() =>
  props.draft.resources?.cpuMillis != null ? formatCpu(props.draft.resources.cpuMillis) : "",
);

function send(adminPassword: string | null): Promise<ActReport> {
  return props.perform(adminPassword, {
    reservedPorts: reservedOk.value,
    lowDisk: diskOk.value,
  });
}
</script>

<template>
  <AdminActDialog
    :open="open"
    :server-id="serverId"
    kind="service_deploy"
    :title="t('deploy.summaryTitle')"
    :submit-label="t('deploy.summarySubmit')"
    :cancel-label="t('deploy.summaryEdit')"
    :can-submit="attentionDone"
    :perform="send"
    :refusal-text="refusalText"
    wide
    @close="emit('close')"
  >
    <dl class="sum" data-deploy-summary>
      <div class="sum__row">
        <dt>{{ t("deploy.summaryName") }}</dt>
        <dd class="sum__mono" data-sum-name>{{ draft.name }}</dd>
      </div>
      <div class="sum__row">
        <dt>{{ t("deploy.summaryImage") }}</dt>
        <dd data-sum-image>
          <span class="sum__mono">{{ repository }}:{{ version.tag }}</span>
          <HTag v-if="version.byDefault" tone="neutral">{{ t("deploy.summaryDefault") }}</HTag>
        </dd>
      </div>
      <div class="sum__row">
        <dt>{{ t("deploy.summaryPorts") }}</dt>
        <dd data-sum-ports>
          <template v-if="draft.ports.length > 0">
            <ul class="sum__list">
              <li v-for="port in draft.ports" :key="port.id" class="sum__mono">
                {{
                  t("deploy.summaryPortRow", {
                    host: port.host,
                    container: port.container,
                    protocol: protocolText(port.protocol),
                  })
                }}
              </li>
            </ul>
            <span class="sum__note" data-sum-exposed>{{ t("deploy.portExposed") }}</span>
          </template>
          <template v-else>{{ t("deploy.summaryNoPorts") }}</template>
        </dd>
      </div>
      <div class="sum__row">
        <dt>{{ t("deploy.summaryEnv") }}</dt>
        <dd data-sum-env>
          <template v-if="envCount > 0">
            {{ envCount === 1 ? t("deploy.summaryEnvCountOne") : t("deploy.summaryEnvCount", { n: envCount }) }}
            <ul class="sum__list">
              <li v-for="row in draft.env" :key="row.id" class="sum__mono">
                {{ row.name }} <span aria-hidden="true">{{ t("deploy.summaryMask") }}</span>
              </li>
            </ul>
          </template>
          <template v-else>{{ t("deploy.summaryNoEnv") }}</template>
        </dd>
      </div>
      <div class="sum__row">
        <dt>{{ t("deploy.summaryCommand") }}</dt>
        <dd data-sum-command>
          {{ draft.command.trim() === "" ? t("deploy.summaryCommandImage") : t("deploy.summaryCommandCustom") }}
        </dd>
      </div>
      <div class="sum__row">
        <dt>{{ t("deploy.summaryRestart") }}</dt>
        <dd data-sum-restart>{{ t("deploy.restartText") }}</dd>
      </div>
      <div class="sum__row">
        <dt>{{ t("deploy.summaryResources") }}</dt>
        <dd data-sum-resources>{{ t("deploy.summaryResourcesText", { memory, cpu }) }}</dd>
      </div>
    </dl>

    <Callout v-if="version.byDefault" tone="info" inline data-sum-latest>
      {{ t("deploy.tagLatest") }}
    </Callout>
    <Callout v-if="imagePresent" tone="info" inline data-sum-present>
      {{ t("deploy.imagePresent") }}
      <template v-if="version.tag === 'latest'"> {{ t("deploy.imagePresentLatest") }}</template>
    </Callout>

    <section v-if="needsAttention" class="attention" :aria-label="t('deploy.attentionTitle')" data-sum-attention>
      <div v-if="reservedPorts.length > 0" class="attention__item" data-attention="reserved">
        <Callout tone="warn" inline>
          <template v-for="port in reservedPorts" :key="`${port.protocol}/${port.hostPort}`">
            <span class="attention__line">
              {{ portVerdict(port).warning }}
            </span>
          </template>
        </Callout>
        <HCheckbox v-model="reservedOk" :label="t('deploy.attentionReserved')" />
      </div>
      <div v-if="lowDisk" class="attention__item" data-attention="disk">
        <Callout tone="warn" inline>{{ lowDiskText }}</Callout>
        <HCheckbox v-model="diskOk" :label="t('deploy.attentionDisk')" />
      </div>
    </section>
    <p v-if="needsAttention && !attentionDone" class="sum__need" data-sum-need>
      {{ t("deploy.summaryAttentionNeeded") }}
    </p>
  </AdminActDialog>
</template>

<style scoped>
.sum {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  margin: 0;
}

.sum__row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 3fr);
  gap: var(--space-3);
}

.sum__row dt {
  color: var(--tx-secondary);
}

.sum__row dd {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: var(--space-2);
  min-width: 0;
  margin: 0;
  overflow-wrap: anywhere;
}

.sum__mono {
  font-family: var(--font-mono);
}

.sum__list {
  display: flex;
  flex-direction: column;
  flex-basis: 100%;
  gap: var(--space-1);
  padding: 0;
  margin: 0;
  list-style: none;
}

.sum__note {
  flex-basis: 100%;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.attention {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.attention__item {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.attention__line {
  display: block;
}

.sum__need {
  margin: 0;
  color: var(--tx-secondary);
}
</style>
