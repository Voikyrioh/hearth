<script setup lang="ts">
import { computed, ref } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HCheckbox from "@/components/atoms/HCheckbox.vue";
import HTag from "@/components/atoms/HTag.vue";
import Callout from "@/components/molecules/Callout.vue";
import AdminActPanel from "@/components/organisms/AdminActPanel.vue";
import { useAdminAct } from "@/composables/useAdminAct";
import type { ActReport } from "@/composables/useReauth";
import type { Draft } from "@/deploy/draft";
import { shownRepository, shownVersion } from "@/deploy/form";
import { portVerdict, refusalText } from "@/deploy/messages";
import { launchReason } from "@/fiche/review";
import { t } from "@/i18n";
import type { DeployCheck } from "@/link";
import { formatCpu, formatMemory } from "@/resources";
import { formatSize } from "@/serviceOps/messages";

// La vérification d'un déploiement, troisième étape de la PAGE (HRT-246, plus une fenêtre ; HRT-69, BR-DEPLOY-015, 018, 031) :
// tout ce qui va partir ET la confirmation du socle (`AdminActPanel`, acte `service_deploy`, JAMAIS couvert par l'élévation
// de 5 minutes : le mot de passe est demandé même juste après un autre acte). Elle montre le nom, l'image et la version, les
// ports (et qui peut les joindre), le nombre de variables (leurs valeurs masquées, jamais en clair), la commande oui ou non,
// le redémarrage, les ressources, puis les POINTS D'ATTENTION qui s'appliquent : chacun une case, et « Déployer le service »
// reste inactif tant qu'un point n'est pas validé ou que le mot de passe manque. Les points viennent de la consultation
// faite à l'arrivée (`check`) ; le serveur les refait au lancement et a le dernier mot (BR-DEPLOY-032). « Modifier » revient
// au formulaire intact. La commande n'est pas répétée ici : la page dit seulement si elle est personnalisée.
const props = defineProps<{
  serverId: string;
  serverName: string;
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
const emit = defineEmits<{
  /** « Modifier » : revient au formulaire, la saisie intacte. */
  leave: [];
  /** L'envoi est parti (accepté, ou résultat inconnu) : la progression prend le relais. */
  launched: [];
}>();

const reservedOk = ref(false);
const diskOk = ref(false);

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

const act = useAdminAct({
  serverId: () => props.serverId,
  kind: () => "service_deploy",
  role: () => null,
  active: () => true,
  canSubmit: () => attentionDone.value,
  blockedReason: () => launchReason(true, false) ?? undefined,
  perform: send,
  refusalText,
  onFinished: () => emit("launched"),
});

const passwordMissing = computed(() => act.askPassword.value && act.password.value === "");
/** Pourquoi le bouton est inactif ; `null` : il est actif. */
const reason = computed(() =>
  launchReason(needsAttention.value && !attentionDone.value, passwordMissing.value),
);
</script>

<template>
  <form class="review" data-review data-deploy-review novalidate @submit.prevent="act.canSend.value && act.submit()">
    <p class="review__intro" data-review-intro>
      {{ t("deploy.review.intro", { server: serverName }) }}
    </p>
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

    <section class="review__block" :aria-label="t('fiche.review.blockPassword')">
      <h2 class="review__title">{{ t("fiche.review.blockPassword") }}</h2>
      <fieldset class="review__fieldset">
        <AdminActPanel :act="act" :server-id="serverId" />
      </fieldset>
      <p class="review__note" data-review-password-help>{{ t("deploy.review.passwordHelp") }}</p>
      <Callout v-if="act.error.value" tone="error" role="alert" data-review-error>
        {{ act.error.value }}
      </Callout>
    </section>

    <div class="review__bar" data-review-bar>
      <HButton variant="ghost" size="lg" data-review-back @click="emit('leave')">
        {{ t("deploy.review.back") }}
      </HButton>
      <p v-if="reason" class="review__reason" data-review-reason>{{ reason }}</p>
      <HButton
        type="submit"
        size="lg"
        data-review-launch
        :disabled="!act.canSend.value"
        :busy="act.sending.value"
      >
        {{ t("deploy.summarySubmit") }}
      </HButton>
    </div>
  </form>
</template>

<style scoped>
.review {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

.review__intro {
  margin: 0;
  color: var(--tx-secondary);
}

.review__block {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  min-width: 0;
  padding: var(--space-4) var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.review__title {
  margin: 0;
  font-size: var(--fs-lead);
  font-weight: var(--fw-semibold);
}

.review__note {
  margin: 0;
  color: var(--tx-secondary);
}

.review__fieldset {
  min-width: 0;
  margin: 0;
  padding: 0;
  border: 0;
}

.review__bar {
  /* À la suite du mot de passe, jamais collée en bas de la fenêtre : elle ne cache plus l'erreur sous le champ. */
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.review__reason {
  margin: 0;
  min-width: 0;
  color: var(--tx-secondary);
}

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
