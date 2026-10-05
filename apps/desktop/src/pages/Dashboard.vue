<script setup lang="ts">
import ComingSoonPanel from "@/components/molecules/ComingSoonPanel.vue";
import { useCurrentServer } from "@/composables/useCurrentServer";

// Tableau de bord : « Bientôt disponible » (son ticket suit).
const { isConnected, lastContactAt } = useCurrentServer();
</script>

<template>
  <ComingSoonPanel :stale="!isConnected" :last-contact-at="lastContactAt" />
</template>
