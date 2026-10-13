<script setup lang="ts">
import { computed } from "vue";
import { t } from "@/i18n";
import type { ServicePort } from "@/link";
import { portLabel } from "@/services/format";

// Les ports exposés d'un service : au plus 3 puces « {port du serveur} vers {port du service} ({protocole}) »,
// puis « +N » (BR-SVC-002). Aucun port : « Aucun port ». Un service sur le réseau du serveur n'a pas de port
// individuel (BR-SVC-014) : rien n'est inventé.
const props = defineProps<{ ports: readonly ServicePort[] }>();

const MAX = 3;
const shown = computed(() => props.ports.slice(0, MAX));
const more = computed(() => Math.max(0, props.ports.length - MAX));
</script>

<template>
  <span v-if="ports.length === 0" class="chips chips--none">{{ t("services.noPort") }}</span>
  <span v-else class="chips">
    <span v-for="(port, index) in shown" :key="index" class="chip">{{ portLabel(port) }}</span>
    <span v-if="more > 0" class="chip chip--more">{{ t("services.portMore", { n: more }) }}</span>
  </span>
</template>

<style scoped>
.chips {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-1);
  min-width: 0;
}

.chips--none {
  color: var(--tx3);
}

.chip {
  padding: var(--space-half) var(--space-2);
  border-radius: var(--radius-pill);
  background: var(--bg);
  color: var(--tx);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.chip--more {
  border: var(--border-width) solid var(--bd);
  background: transparent;
  color: var(--tx2);
}
</style>
