<script setup lang="ts">
import { useRouter } from "vue-router";
import AddServerWizard from "@/components/organisms/AddServerWizard.vue";

// Ajout d'un serveur : carte centrée, sans navigation de serveur. Annuler ou refuser l'empreinte
// ramène à l'accueil (liste des serveurs ou écran de premier lancement) ; une connexion réussie
// ouvre le tableau de bord du nouveau serveur.
const router = useRouter();

function cancel() {
  void router.push({ name: "home" });
}

function done(serverId: string) {
  void router.push({ name: "dashboard", params: { id: serverId } });
}
</script>

<template>
  <main class="add">
    <AddServerWizard @cancel="cancel" @done="done" />
  </main>
</template>

<style scoped>
.add {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  padding: var(--space-5);
  overflow-y: auto;
}
</style>
