<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import { t } from "@/i18n";

// La barre « N changements prêts » (HRT-262, maquette Main révisée) : ce que le lot contient, « Tout annuler » et « Vérifier
// et appliquer ». Rien n'est fait tant que le compte n'a pas appliqué (écran « Vérifiez avant d'appliquer », HRT-263).
defineProps<{ count: number; busy?: boolean; blockedReason?: string }>();
const emit = defineEmits<{ cancelAll: []; review: [] }>();
</script>

<template>
  <div class="bar" role="status" data-mods-pending-bar>
    <HIcon name="clock" size="sm" />
    <span class="bar__text">
      <strong>{{ t(count === 1 ? "mods.bar.one" : "mods.bar.many", { n: count }) }}</strong>
      {{ t("mods.bar.text") }}
    </span>
    <span class="bar__actions">
      <HButton variant="secondary" size="sm" data-mods-cancel-all @click="emit('cancelAll')">
        {{ t("mods.bar.cancelAll") }}
      </HButton>
      <HButton
        variant="primary"
        size="sm"
        needs-link
        :disabled="blockedReason !== undefined"
        :hint="blockedReason"
        :busy="busy"
        data-mods-review
        @click="emit('review')"
      >
        {{ t("mods.bar.apply") }}
      </HButton>
    </span>
  </div>
</template>

<style scoped>
.bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-4);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-card);
  background: var(--card);
}

.bar__text {
  flex: 1 1 16rem;
  min-width: 0;
}

.bar__actions {
  display: flex;
  gap: var(--space-2);
}
</style>
