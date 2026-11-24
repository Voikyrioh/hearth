<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import { t } from "@/i18n";

// La barre « N changements prêts » (HRT-262, maquette Main révisée) : ce que le lot contient, « Tout annuler » et « Vérifier
// et appliquer ». Rien n'est fait tant que le compte n'a pas appliqué (écran « Vérifiez avant d'appliquer », HRT-263).
defineProps<{
  count: number;
  /** Quand le lot ne compte qu'un changement : ce changement, en une phrase (« Simple Voice Chat sera désactivé »). */
  sentence?: string;
  busy?: boolean;
  blockedReason?: string;
}>();
const emit = defineEmits<{ cancelAll: []; review: [] }>();
</script>

<template>
  <div class="bar" role="status" data-mods-pending-bar>
    <HIcon name="clock" size="sm" />
    <span class="bar__text">
      <strong>{{ t(count === 1 ? "mods.bar.one" : "mods.bar.many", { n: count }) }}</strong>{{ sentence ? t("mods.bar.textNamed", { sentence }) : t("mods.bar.text") }}
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
/* Colle à la zone qui défile : sous l'en-tête collant quand on a dépassé sa place, en bas quand elle est plus bas que
   l'écran. Le lot et « Vérifier et appliquer » restent à portée sur une longue liste. */
.bar {
  position: sticky;
  top: calc(var(--mods-pinned-h) + var(--space-2));
  bottom: var(--space-3);
  z-index: var(--z-raised);
  box-shadow: var(--card-edge);
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
  flex: 1 1 var(--card-min);
  min-width: 0;
}

.bar__actions {
  display: flex;
  gap: var(--space-2);
}
</style>
