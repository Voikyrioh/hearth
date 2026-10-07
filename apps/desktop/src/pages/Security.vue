<script setup lang="ts">
import { computed, ref, watch } from "vue";
import RemoveDeviceDialog from "@/components/organisms/RemoveDeviceDialog.vue";
import TrustedDeviceTable from "@/components/organisms/TrustedDeviceTable.vue";
import { useCurrentServer } from "@/composables/useCurrentServer";
import type { TrustedDevice } from "@/link";
import { useDevicesStore } from "@/stores/devices";
import { useLinkStore } from "@/stores/link";

// Sécurité d'un serveur : « Tes postes de confiance » (HRT-23). Ouverte à tous les rôles : chacun ne
// voit et ne retire que SES postes (l'agent est l'arbitre). La liste se relit à chaque retour du lien et
// à chaque issue d'opération incertaine : jamais d'état supposé. Le retrait passe par une fenêtre qui
// demande le mot de passe (acte d'administration) ; la preuve de la clé de ce PC est faite par la
// coquille, la page ne la voit pas. L'emplacement du panneau « Mode attaque » (HRT-26) est
// au-dessus de la liste : rien n'y est affiché tant que ce panneau n'existe pas.
const { server, state } = useCurrentServer();
const store = useDevicesStore();
const link = useLinkStore();
const serverId = computed(() => server.value?.id ?? "");

const entry = computed(() => store.of(serverId.value));

watch(
  serverId,
  (id) => {
    if (id) void store.load(id);
  },
  { immediate: true },
);
// Retour du lien : l'état réel du serveur (BR-RESIL-010).
watch(state, (now, before) => {
  if (now === "connected" && before !== "connected" && serverId.value) {
    void store.load(serverId.value);
  }
});
// Une issue d'opération connue (« fait pendant la coupure », « non exécuté ») : relire.
watch(
  () => Object.keys(link.operations).length,
  () => {
    if (serverId.value) void store.load(serverId.value);
  },
);

const removing = ref<TrustedDevice | null>(null);
</script>

<template>
  <div class="security">
    <TrustedDeviceTable
      :status="entry?.status ?? 'loading'"
      :devices="entry?.devices ?? []"
      :max="entry?.max ?? 8"
      @remove="removing = $event"
      @retry="store.load(serverId)"
    />
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
