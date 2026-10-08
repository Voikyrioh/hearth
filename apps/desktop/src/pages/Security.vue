<script setup lang="ts">
import { computed, ref, watch } from "vue";
import AttackModePanel from "@/components/organisms/AttackModePanel.vue";
import ReauthSettingCard from "@/components/organisms/ReauthSettingCard.vue";
import RemoveDeviceDialog from "@/components/organisms/RemoveDeviceDialog.vue";
import SecurityAlertCard from "@/components/organisms/SecurityAlertCard.vue";
import TrustedDeviceTable from "@/components/organisms/TrustedDeviceTable.vue";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { useResumeMinutes } from "@/composables/useResumeMinutes";
import { reportUiError } from "@/errors/report";
import { type AttackMode, alertVisible, getLinkBridge, type TrustedDevice } from "@/link";
import { attackModeBlock } from "@/security/gate";
import { useDevicesStore } from "@/stores/devices";
import { useLinkStore } from "@/stores/link";
import { useSecurityStore } from "@/stores/security";

// Sécurité d'un serveur : « Mode attaque » (HRT-26) puis « Tes postes de confiance » (HRT-23). Ouverte à
// tous les rôles : chacun voit l'alerte et ses postes (l'agent est l'arbitre). Les bandeaux d'alerte et
// de mode attaque sont posés par le gabarit du serveur, hors de cette page. L'état se relit à chaque
// retour du lien et à chaque issue d'opération incertaine : jamais d'état supposé. Activer, désactiver
// et retirer un poste passent par une fenêtre qui demande le mot de passe (acte d'administration) ; la
// preuve de la clé de ce PC est faite par la coquille, la page ne la voit pas.
const { server, state } = useCurrentServer();
const store = useDevicesStore();
const security = useSecurityStore();
const link = useLinkStore();
const serverId = computed(() => server.value?.id ?? "");

const entry = computed(() => store.of(serverId.value));
const securityEntry = computed(() => security.of(serverId.value));
const mode = computed<AttackMode>(
  () =>
    securityEntry.value?.state?.attackMode ?? {
      state: "off",
      since: null,
      resumesInS: null,
      lastEnd: null,
    },
);
const block = computed(() => attackModeBlock(server.value?.role, securityEntry.value));
const minutes = useResumeMinutes(() => securityEntry.value);
const busy = computed(() => security.dialog?.serverId === serverId.value);

watch(
  serverId,
  (id) => {
    if (!id) return;
    void store.load(id);
    void security.load(id);
  },
  { immediate: true },
);
// Retour du lien : l'état réel du serveur (BR-RESIL-010).
watch(state, (now, before) => {
  if (now === "connected" && before !== "connected" && serverId.value) {
    void store.load(serverId.value);
    void security.load(serverId.value);
  }
});
// Une issue d'opération connue (« fait pendant la coupure », « non exécuté ») : relire.
watch(
  () => Object.keys(link.operations).length,
  () => {
    if (!serverId.value) return;
    void store.load(serverId.value);
    void security.load(serverId.value);
  },
);

const removing = ref<TrustedDevice | null>(null);

function change() {
  security.ask(serverId.value, mode.value.state === "off");
}

// Poste sans clé enregistrée et sans mot de passe mémorisé : se reconnecter par mot de passe l'inscrit.
// La déconnexion volontaire ramène le formulaire de connexion (le mot de passe mémorisé, s'il y en a un,
// est conservé : cette voie ne sert que sans lui).
async function reconnect() {
  try {
    await getLinkBridge().logout(serverId.value);
  } catch (error) {
    reportUiError(error, "security:reconnect");
  }
}
</script>

<template>
  <div class="security">
    <SecurityAlertCard
      v-if="securityEntry?.state && alertVisible(securityEntry.state.alert)"
      :alert="securityEntry.state.alert"
      :role="server?.role ?? 'readonly'"
      :server-id="serverId"
      :mode-on="mode.state !== 'off'"
    />
    <AttackModePanel
      :mode="mode"
      :block="block"
      :minutes="minutes"
      :busy="busy"
      :can-reconnect="!server?.remember"
      @change="change"
      @reconnect="reconnect"
      @retry="security.load(serverId)"
    />
    <TrustedDeviceTable
      :status="entry?.status ?? 'loading'"
      :devices="entry?.devices ?? []"
      :max="entry?.max ?? 8"
      @remove="removing = $event"
      @retry="store.load(serverId)"
    />
    <ReauthSettingCard :key="serverId" :server-id="serverId" />
    <RemoveDeviceDialog
      :open="removing !== null"
      :server-id="serverId"
      :device="removing"
      @close="removing = null"
    />
  </div>
</template>

<style scoped>
.security {
  display: flex;
  flex-direction: column;
  gap: var(--card-gap);
  max-width: var(--column-max);
}
</style>
