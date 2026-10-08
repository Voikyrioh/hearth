<script setup lang="ts">
import { computed } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HTag from "@/components/atoms/HTag.vue";
import { formatDateTime } from "@/composables/format";
import { useNotLoadedText } from "@/composables/useNotLoadedText";
import { t } from "@/i18n";
import type { AttackMode } from "@/link";
import { type AttackModeBlock, blockMessageKey } from "@/security/gate";

// Carte « Mode attaque » de la page Sécurité (BR-TRUST-010, 018, 028, 029) : l'état écrit (« Inactif »,
// « Actif », « Suspendu »), l'explication, et UN bouton qui active ou désactive. Le bouton reste
// focusable quand il est indisponible : son infobulle reprend la raison, ET la raison est écrite sous le
// bouton (une infobulle seule ne suffit ni à un lecteur d'écran ni au toucher). Trois raisons, une seule
// affichée : Lecture seule, agent trop ancien, poste sans clé enregistrée. La désactivation par le lien
// garde la mécanique `needs-link`. Le panneau ne décide rien : l'agent arbitre ; il demande la
// confirmation (`change`) et la page ouvre la fenêtre.
const props = defineProps<{
  mode: AttackMode;
  block: AttackModeBlock | null;
  /** Minutes avant la reprise (0 : moins d'une minute) ; seulement suspendu. */
  minutes: number | null;
  /** Une action est en cours : le bouton attend. */
  busy?: boolean;
  /** Le bouton « Se reconnecter » : seulement pour un poste sans clé et sans mot de passe mémorisé. */
  canReconnect?: boolean;
}>();

defineEmits<{ change: []; reconnect: []; retry: [] }>();

// HRT-38 (C46) : hors ligne, « état illisible » est faux et « Réessayer » double celui du bandeau : la carte dit
// seulement que l'état n'est pas encore chargé.
const notLoadedText = useNotLoadedText();
const notLoaded = computed(() => props.block === "unreadable" && notLoadedText.value !== null);

const on = computed(() => props.mode.state !== "off");
const suspended = computed(() => props.mode.state === "suspended");
const reason = computed(() => {
  if (notLoaded.value) return notLoadedText.value ?? undefined;
  const key = blockMessageKey(props.block);
  return key ? t(key) : undefined;
});
const stateLabel = computed(() =>
  t(
    suspended.value
      ? "security.stateSuspended"
      : on.value
        ? "security.stateOn"
        : "security.stateOff",
  ),
);
const tagTone = computed(() => (on.value && !suspended.value ? "accent" : "neutral"));
const text = computed(() => {
  if (!suspended.value) {
    if (!on.value) return t("security.modeTextOff");
    return props.mode.since
      ? t("security.modeTextOnSince", { time: formatDateTime(props.mode.since) })
      : t("security.modeTextOn");
  }
  return props.minutes === null || props.minutes === 0
    ? t("security.suspendedSoon")
    : t("security.suspendedText", { n: props.minutes });
});
</script>

<template>
  <section
    class="panel"
    aria-labelledby="attack-mode-title"
    :aria-busy="busy ? 'true' : undefined"
    data-attack-mode-panel
  >
    <header class="panel__head">
      <h2 id="attack-mode-title" class="panel__title">{{ t("security.panelTitle") }}</h2>
      <HTag :tone="tagTone" data-attack-mode-state>
        <HIcon name="shield" size="sm" class="panel__tag-icon" />
        {{ stateLabel }}
      </HTag>
    </header>
    <p class="panel__text">{{ text }}</p>
    <details v-if="on" class="panel__more" data-attack-mode-details>
      <summary>{{ t("security.howItWorks") }}</summary>
      <p class="panel__note">{{ t("security.autoEndNote") }}</p>
      <ul class="panel__notes">
        <li>{{ t("security.attackNoteAddress") }}</li>
        <li>{{ t("security.attackNoteChallenge") }}</li>
      </ul>
    </details>
    <div class="panel__actions">
      <HButton
        :variant="on ? 'secondary' : 'primary'"
        needs-link
        :disabled="block !== null"
        :busy="busy"
        :hint="reason"
        data-attack-mode-toggle
        @click="$emit('change')"
      >
        {{ t(on ? "security.deactivate" : "security.activate") }}
      </HButton>
      <HButton
        v-if="block === 'unreadable' && !notLoaded"
        variant="secondary"
        data-security-retry
        @click="$emit('retry')"
      >
        {{ t("common.retry") }}
      </HButton>
      <HButton
        v-if="block === 'not_enrolled' && canReconnect"
        variant="secondary"
        data-security-reconnect
        @click="$emit('reconnect')"
      >
        {{ t("security.reconnect") }}
      </HButton>
    </div>
    <p
      v-if="reason"
      :class="['panel__reason', { 'panel__reason--warn': block === 'not_enrolled' }]"
      data-attack-mode-reason
    >
      <HIcon
        :name="block === 'not_enrolled' || block === 'unreadable' ? 'alert' : 'info'"
        size="sm"
        class="panel__reason-icon"
      />
      <span>{{ reason }}</span>
    </p>
  </section>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.panel__more summary {
  cursor: pointer;
  color: var(--ac);
}

.panel__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}

.panel__title {
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.panel__tag-icon {
  display: inline-block;
  vertical-align: middle;
}

.panel__text {
  color: var(--tx2);
}

.panel__note {
  color: var(--tx2);
}

.panel__notes {
  padding-left: var(--space-4);
  color: var(--tx2);
  list-style: disc;
}

.panel__notes li + li {
  margin-top: var(--space-2);
}

.panel__actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
}

.panel__reason {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  color: var(--tx2);
}

.panel__reason--warn .panel__reason-icon {
  color: var(--warn);
}

.panel__reason-icon {
  margin-top: var(--space-half);
}
</style>
