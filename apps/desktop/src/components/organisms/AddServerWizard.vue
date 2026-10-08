<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { RouterLink } from "vue-router";
import HButton from "@/components/atoms/HButton.vue";
import HInput from "@/components/atoms/HInput.vue";
import ColorSwatches from "@/components/molecules/ColorSwatches.vue";
import FingerprintBlock from "@/components/molecules/FingerprintBlock.vue";
import StepTrail from "@/components/molecules/StepTrail.vue";
import LoginForm from "@/components/organisms/LoginForm.vue";
import { ADD_STEPS, useAddServer } from "@/composables/useAddServer";
import { t } from "@/i18n";

// Assistant d'ajout de serveur en 3 temps (adresse, empreinte, connexion). La logique est dans
// `useAddServer` ; ici, seulement l'affichage. « Refuser » et « Annuler » rendent la main à la
// page par `cancel` ; la connexion réussie par `done` avec l'identifiant du serveur.
const emit = defineEmits<{ cancel: []; done: [serverId: string] }>();

const wizard = useAddServer();
const login = ref<InstanceType<typeof LoginForm> | null>(null);
const root = ref<HTMLElement | null>(null);

const stepIndex = computed(() => ADD_STEPS.indexOf(wizard.step.value));
const stepNames = computed(() => [
  t("connect.stepAddress"),
  t("connect.stepFingerprint"),
  t("connect.stepLogin"),
]);
const checking = computed(() => wizard.busy.value === "probe");

async function onLogin(entry: { username: string; password: string; remember: boolean }) {
  const id = await wizard.login(entry);
  if (id) emit("done", id);
  else login.value?.clearPassword();
}

// FIX:01M4D0RJ5EMX3TG1TJB0EJ5EYP (C2) : à chaque étape qui a un champ, le curseur est dans le premier.
async function focusFirstField() {
  await nextTick();
  root.value?.querySelector<HTMLElement>("input:not([type=hidden]):not(:disabled)")?.focus();
}
onMounted(focusFirstField);
watch(() => wizard.step.value, focusFirstField);

// FIX:01M4D0RJ5EMX3TG1TJB0EJ5EYP (C1) : « Suivant » n'est jamais grisé sans raison. Un champ manquant ou
// faux est dit sous le champ et le curseur y va, au clic comme à Entrée.
async function submitAddress() {
  await wizard.next();
  await nextTick();
  root.value?.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
}

// Rien n'existe tant que la connexion n'a pas réussi : annuler ne défait rien.
function cancel() {
  emit("cancel");
}

const address = computed(() => {
  const port = wizard.port.value.trim();
  return port ? `${wizard.host.value.trim()}:${port}` : wizard.host.value.trim();
});

function refuse() {
  wizard.refuse();
  emit("cancel");
}
</script>

<template>
  <section ref="root" class="wizard" :aria-labelledby="'wizard-title'">
    <StepTrail :steps="stepNames" :current="stepIndex" :label="t('connect.stepsLabel')" />

    <!-- Temps 1 : nom, adresse, port, couleur -->
    <form
      v-if="wizard.step.value === 'address'"
      class="wizard__form"
      novalidate
      @submit.prevent="submitAddress"
    >
      <h1 id="wizard-title" class="wizard__title">{{ t("connect.addTitle") }}</h1>
      <HInput
        v-model="wizard.name.value"
        :label="t('connect.name')"
        :placeholder="t('connect.namePlaceholder')"
        :disabled="checking"
        :error="wizard.errors.value.name ?? undefined"
        @update:model-value="wizard.edited(); wizard.touched.value.name = true"
      />
      <div class="wizard__row">
        <div class="wizard__host">
          <HInput
            v-model="wizard.host.value"
            :label="t('connect.host')"
            :placeholder="t('connect.hostPlaceholder')"
            :disabled="checking"
            :error="wizard.errors.value.host ?? undefined"
            @update:model-value="wizard.edited(); wizard.touched.value.host = true"
          />
        </div>
        <div class="wizard__port">
          <HInput
            v-model="wizard.port.value"
            :label="t('connect.port')"
            :placeholder="t('connect.portPlaceholder')"
            :disabled="checking"
            :error="wizard.errors.value.port ?? undefined"
            mono
            @update:model-value="wizard.edited(); wizard.touched.value.port = true"
          />
        </div>
      </div>
      <div class="wizard__colors">
        <span class="wizard__colors-label">{{ t("connect.color") }}</span>
        <ColorSwatches v-model="wizard.color.value" :label="t('connect.color')" :disabled="checking" />
      </div>
      <p v-if="wizard.existing.value" class="wizard__info" role="status" data-existing>
        {{ t("connect.existing") }}
        <RouterLink
          class="wizard__link"
          :to="{ name: 'dashboard', params: { id: wizard.existing.value.id } }"
        >
          {{ t("connect.openExisting") }}
        </RouterLink>
      </p>
      <p v-if="wizard.cardMessage.value" class="wizard__error" role="alert">
        {{ wizard.cardMessage.value }}
      </p>
      <p v-if="checking" class="wizard__status" role="status">{{ t("connect.checking") }}</p>
      <div class="wizard__actions">
        <HButton variant="secondary" :disabled="checking" @click="cancel">
          {{ t("connect.cancel") }}
        </HButton>
        <HButton type="submit" :busy="checking">
          {{ t("connect.next") }}
        </HButton>
      </div>
    </form>

    <!-- Temps 2 : l'empreinte à comparer -->
    <div v-else-if="wizard.step.value === 'fingerprint' && wizard.probe.value" class="wizard__form">
      <h1 id="wizard-title" class="wizard__title">{{ t("connect.verifyTitle") }}</h1>
      <!-- FIX:01M4E5D5Z5MBSD639B5NPX81GC (C3) : le serveur qu'on vérifie est rappelé, avec un retour en arrière et la conséquence d'un refus. -->
      <p class="wizard__server" data-fingerprint-server>
        <strong>{{ wizard.name.value }}</strong>
        <span class="wizard__address">{{ address }}</span>
      </p>
      <FingerprintBlock
        :value="wizard.probe.value.display"
        :label="t('connect.fingerprintLabel')"
      />
      <p class="wizard__help">{{ t("connect.fingerprintHelp") }}</p>
      <p class="wizard__help" data-fingerprint-where>{{ t("connect.fingerprintWhere") }}</p>
      <p class="wizard__help" data-fingerprint-refuse>{{ t("connect.fingerprintRefuseHelp") }}</p>
      <div class="wizard__actions">
        <HButton variant="ghost" data-fingerprint-back @click="wizard.back()">
          {{ t("connect.previous") }}
        </HButton>
        <HButton variant="secondary" @click="refuse">
          {{ t("connect.refuseFingerprint") }}
        </HButton>
        <HButton @click="wizard.confirm()">
          {{ t("connect.confirmFingerprint") }}
        </HButton>
      </div>
    </div>

    <!-- Temps 3 : connexion -->
    <div v-else class="wizard__form">
      <h1 id="wizard-title" class="wizard__title">{{ t("connect.loginTitle") }}</h1>
      <LoginForm
        ref="login"
        :busy="wizard.busy.value === 'login'"
        :error="wizard.loginError.value"
        :locked-seconds="wizard.lockedSeconds.value"
        :back-label="t('connect.previous')"
        @submit="onLogin"
        @back="wizard.back()"
      />
    </div>
  </section>
</template>

<style scoped>
.wizard {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
  width: 100%;
  max-width: var(--wizard-width);
  padding: var(--space-5);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.wizard__form {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.wizard__title {
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.wizard__row {
  display: flex;
  gap: var(--space-3);
}

.wizard__host {
  flex: 3;
}

.wizard__port {
  flex: 1;
}

.wizard__colors {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.wizard__colors-label {
  color: var(--tx2);
  font-size: var(--fs-small);
}

.wizard__help,
.wizard__status {
  color: var(--tx2);
}

.wizard__info {
  color: var(--tx2);
}

.wizard__link {
  color: var(--ac);
  text-decoration: underline;
}

.wizard__error {
  color: var(--crit);
}

.wizard__server {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: var(--space-2) var(--space-3);
}

.wizard__address {
  color: var(--tx2);
  font-family: var(--font-mono);
}

.wizard__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-3);
}
</style>
