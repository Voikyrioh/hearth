<script setup lang="ts">
import { computed, ref } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HInput from "@/components/atoms/HInput.vue";
import ColorSwatches from "@/components/molecules/ColorSwatches.vue";
import FingerprintBlock from "@/components/molecules/FingerprintBlock.vue";
import { t } from "@/i18n";
import {
  DEFAULT_PORT,
  failureMessage,
  failureOf,
  getLinkBridge,
  type ProbeResult,
  type ServerColor,
  type ServerInfo,
} from "@/link";
import { useServersStore } from "@/stores/servers";
import { hostError, nameError, parsePort, portError } from "@/validation/server";

// Modification d'un serveur enregistré : nom, couleur, adresse. Si l'adresse change, l'empreinte
// du serveur trouvé à la nouvelle adresse doit être relue et confirmée de nouveau avant d'enregistrer
// (BR-CONN-009) ; les identifiants mémorisés sont conservés.
const props = defineProps<{ server: ServerInfo }>();
const emit = defineEmits<{ done: [server: ServerInfo]; cancel: [] }>();

const servers = useServersStore();
const name = ref(props.server.name);
const host = ref(props.server.host);
const port = ref(props.server.port === DEFAULT_PORT ? "" : String(props.server.port));
const color = ref<ServerColor>(props.server.color);
const step = ref<"form" | "verify">("form");
const probe = ref<ProbeResult | null>(null);
const busy = ref(false);
const failure = ref<string | null>(null);

const others = computed(() => servers.servers.filter((server) => server.id !== props.server.id));
const nameMessage = computed(() => {
  const key = nameError(name.value, others.value);
  return key ? t(key) : undefined;
});
const hostMessage = computed(() => {
  const key = hostError(host.value);
  return key ? t(key) : undefined;
});
const portMessage = computed(() => {
  const key = portError(port.value);
  return key ? t(key) : undefined;
});
const valid = computed(() => !nameMessage.value && !hostMessage.value && !portMessage.value);
const portNumber = computed(() => parsePort(port.value) ?? null);
const moved = computed(
  () =>
    host.value.trim().toLowerCase() !== props.server.host.toLowerCase() ||
    (portNumber.value ?? DEFAULT_PORT) !== props.server.port,
);

async function save(fingerprint: string | null) {
  const updated = await getLinkBridge().updateServer(props.server.id, {
    name: name.value.trim(),
    color: color.value,
    host: host.value.trim(),
    port: portNumber.value,
    fingerprint,
  });
  emit("done", updated);
}

async function submit() {
  if (!valid.value || busy.value) return;
  failure.value = null;
  busy.value = true;
  try {
    if (moved.value) {
      probe.value = await getLinkBridge().probeServer(host.value.trim(), portNumber.value);
      step.value = "verify";
    } else {
      await save(null);
    }
  } catch (error) {
    const reason = failureOf(error);
    failure.value = reason ? failureMessage(reason) : t("failure.generic");
  } finally {
    busy.value = false;
  }
}

async function confirm() {
  if (!probe.value || busy.value) return;
  failure.value = null;
  busy.value = true;
  try {
    await save(probe.value.fingerprint);
  } catch (error) {
    const reason = failureOf(error);
    failure.value = reason ? failureMessage(reason) : t("failure.generic");
    step.value = "form";
  } finally {
    busy.value = false;
  }
}

function refuse() {
  probe.value = null;
  step.value = "form";
}
</script>

<template>
  <section class="edit" :aria-label="t('connect.editTitle')">
    <form v-if="step === 'form'" class="edit__form" novalidate @submit.prevent="submit">
      <h2 class="edit__title">{{ t("connect.editTitle") }}</h2>
      <HInput v-model="name" :label="t('connect.name')" :disabled="busy" :error="nameMessage" />
      <div class="edit__row">
        <div class="edit__host">
          <HInput
            v-model="host"
            :label="t('connect.host')"
            :disabled="busy"
            :error="hostMessage"
          />
        </div>
        <div class="edit__port">
          <HInput
            v-model="port"
            :label="t('connect.port')"
            :placeholder="t('connect.portPlaceholder')"
            :disabled="busy"
            :error="portMessage"
            mono
          />
        </div>
      </div>
      <ColorSwatches v-model="color" :label="t('connect.color')" :disabled="busy" />
      <p v-if="moved" class="edit__info" role="status">{{ t("connect.moved") }}</p>
      <p v-if="failure" class="edit__error" role="alert">{{ failure }}</p>
      <div class="edit__actions">
        <HButton variant="secondary" :disabled="busy" @click="emit('cancel')">
          {{ t("connect.cancel") }}
        </HButton>
        <HButton type="submit" :busy="busy" :disabled="!valid">
          {{ moved ? t("connect.next") : t("connect.save") }}
        </HButton>
      </div>
    </form>

    <div v-else-if="probe" class="edit__form">
      <h2 class="edit__title">{{ t("connect.verifyTitle") }}</h2>
      <FingerprintBlock :value="probe.display" :label="t('connect.fingerprintLabel')" />
      <p class="edit__info">{{ t("connect.fingerprintHelp") }}</p>
      <div class="edit__actions">
        <HButton variant="secondary" :disabled="busy" @click="refuse">
          {{ t("connect.refuseFingerprint") }}
        </HButton>
        <HButton :busy="busy" @click="confirm">{{ t("connect.confirmFingerprint") }}</HButton>
      </div>
    </div>
  </section>
</template>

<style scoped>
.edit {
  padding: var(--space-4);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-card);
  background: var(--card-2);
}

.edit__form {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.edit__title {
  font-size: var(--fs-lead);
  font-weight: var(--fw-semibold);
}

.edit__row {
  display: flex;
  gap: var(--space-3);
}

.edit__host {
  flex: 3;
}

.edit__port {
  flex: 1;
}

.edit__info {
  color: var(--tx2);
}

.edit__error {
  color: var(--crit);
}

.edit__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-3);
}
</style>
