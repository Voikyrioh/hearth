<script setup lang="ts">
import { computed, onBeforeUnmount, useId, watch } from "vue";
import { safeText } from "@/audit/text";
import BoundedText from "@/components/atoms/BoundedText.vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HTag from "@/components/atoms/HTag.vue";
import Callout from "@/components/molecules/Callout.vue";
import DashCard from "@/components/molecules/DashCard.vue";
import ServiceRightReason from "@/components/molecules/ServiceRightReason.vue";
import { t } from "@/i18n";
import { canAct, type RightsGate } from "@/services/rights";
import { useGamePasswordStore } from "@/stores/gamePassword";

// La carte « Mot de passe du serveur » du détail d'un serveur de jeu (HRT-104, BR-JEU-025 à 027). Le mot de passe est masqué
// par un masque FIXE de 8 puces (la longueur réelle ne se devine pas) et ne vient qu'après une demande du compte qui a le
// droit de gérer les services, jamais d'avance : les autres profils lisent « Réservé aux comptes qui peuvent gérer les
// services. », sans bouton. La valeur n'entre dans aucun titre, infobulle, journal, magasin persistant ni presse-papiers
// (aucun bouton « copier » ici). Elle est REMASQUÉE, donc effacée de la mémoire, à la fermeture du détail, au changement
// de service ou d'onglet, quand le lien n'est plus « Connecté », quand le service disparaît, quand le droit est retiré ou
// que le moteur n'est plus prêt. Un serveur sans mot de passe le dit, sans bouton.
const props = defineProps<{
  serverId: string;
  serviceKey: string;
  /** L'agent dit que le serveur a un mot de passe (jamais sa valeur). */
  hasPassword: boolean;
  gate: RightsGate;
  /** L'`id` de la raison écrite (`ServiceRightReason`), liée au bouton désactivé. */
  reasonId: string;
  connected: boolean;
  engineReady: boolean;
  /** Le service n'existe plus : rien ne se révèle. */
  gone: boolean;
}>();
const emit = defineEmits<{ retryRights: [] }>();

const store = useGamePasswordStore();
const noticeId = useId();

const mine = computed(
  () => store.target?.serverId === props.serverId && store.target?.key === props.serviceKey,
);
const phase = computed(() => (mine.value ? store.phase : "idle"));
const shown = computed(() => phase.value === "shown" && store.password !== null);
const reading = computed(() => phase.value === "reading");
const allowed = computed(() => canAct(props.gate));
/** La demande est possible : droit accordé, lien « Connecté », moteur prêt, service présent. */
const possible = computed(
  () => allowed.value && props.connected && props.engineReady && !props.gone,
);
/** Texte permanent qui dit pourquoi le bouton est désactivé quand ce n'est pas le droit (le droit a sa raison). */
const engineReason = computed(() => {
  if (!allowed.value || props.gone) return null;
  if (!props.connected) return t("game.password.offline");
  return props.engineReady ? null : t("game.password.engineNotReady");
});
const rightsReason = computed(() => ["denied", "old-agent", "failed"].includes(props.gate.kind));
/** Le droit n'est pas encore lu ou pas accordé : la carte est celle d'un compte sans le droit. */
const reserved = computed(() => rightsReason.value && props.gate.kind === "denied");
const describedBy = computed(() => {
  if (rightsReason.value) return props.reasonId;
  if (engineReason.value) return noticeId;
  return undefined;
});
const failure = computed(() => (mine.value && phase.value === "failed" ? store.failure : null));
const failureText = computed(() => {
  if (failure.value === "lost") return t("rights.lost");
  if (failure.value === "unreadable") return t("game.password.failed");
  return null;
});
/** La valeur révélée, rendue inoffensive ; `""` : le serveur n'a pas (ou plus) de mot de passe. */
const value = computed(() => (shown.value ? safeText(store.password ?? "", true) : null));
/** Aucun mot de passe : dit par la vue, ou par la lecture quand elle revient vide (BR-JEU-027). */
const none = computed(() => !props.hasPassword || value.value === "");

function toggle() {
  if (shown.value) store.clear();
  else void store.reveal(props.serverId, props.serviceKey);
}
function maskIfMine() {
  if (mine.value) store.clear();
}
watch(
  () => [props.serverId, props.serviceKey],
  () => {
    if (store.target) store.clear();
  },
);
watch(
  () => [props.connected, props.gone, props.engineReady, allowed.value] as const,
  ([connected, gone, ready, ok]) => {
    // Un échec affiché n'est pas une valeur : le message « droit retiré » reste, seule la valeur est effacée.
    if ((!connected || gone || !ready || !ok) && store.phase !== "failed") maskIfMine();
  },
);
onBeforeUnmount(maskIfMine);
</script>

<template>
  <DashCard :title="t('game.password.title')" data-game-password>
    <template v-if="hasPassword && !reserved" #aside>
      <HButton
        variant="secondary"
        size="sm"
        :disabled="!possible && !shown"
        :busy="reading"
        :described-by="describedBy"
        data-game-password-toggle
        @click="toggle"
      >
        <HIcon :name="shown ? 'eye-off' : 'eye'" size="sm" />
        {{
          reading
            ? t("game.password.reading")
            : shown
              ? t("game.password.hide")
              : t("game.password.reveal")
        }}
      </HButton>
    </template>

    <p v-if="reserved" class="pw__note" data-game-password-reserved>
      <HIcon name="lock" size="sm" />
      {{ t("game.password.reserved") }}
    </p>
    <template v-else>
      <Callout v-if="rightsReason" inline data-game-password-reason>
        <ServiceRightReason :gate="gate" :id="reasonId" @retry="emit('retryRights')" />
      </Callout>
      <p v-else-if="engineReason && hasPassword" :id="noticeId" class="pw__note" data-game-password-engine>
        {{ engineReason }}
      </p>

      <Callout v-if="failureText" tone="error" role="alert" inline data-game-password-failed>
        {{ failureText }}
      </Callout>

      <Callout v-if="!hasPassword || (shown && none)" tone="warn" inline data-game-password-none>
        {{ t("game.password.none") }}
      </Callout>
      <template v-else>
        <p class="pw__status" role="status" data-game-password-status>
          <HTag v-if="shown" tone="warn"
            ><HIcon name="eye" size="sm" /> {{ t("game.password.visible") }}</HTag
          >
          <span v-else>{{ t("game.password.masked") }}</span>
        </p>
        <p v-if="value !== null" class="pw__value" data-game-password-value>
          <BoundedText :text="value" :label="t('game.password.value')" />
        </p>
        <p v-else class="pw__mask" data-game-password-mask>
          <span aria-hidden="true">••••••••</span>
          <span class="sr-only">{{ t("game.password.maskedValue") }}</span>
        </p>
      </template>
      <p v-if="hasPassword" class="pw__note" data-game-password-logs>{{ t("game.password.logs") }}</p>
    </template>
  </DashCard>
</template>

<style scoped>
.pw__note,
.pw__status {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  margin: 0;
  color: var(--tx-secondary);
  overflow-wrap: anywhere;
}

.pw__note {
  font-size: var(--fs-small);
}

.pw__mask,
.pw__value {
  margin: 0;
  color: var(--tx);
  font-family: var(--font-mono);
  font-size: var(--fs-h3);
  overflow-wrap: anywhere;
  user-select: text;
}
</style>
