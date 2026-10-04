<script setup lang="ts">
import { RouterLink } from "vue-router";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HLogo from "@/components/atoms/HLogo.vue";
import { t } from "@/i18n";
</script>

<template>
  <nav class="rail" :aria-label="t('rail.label')">
    <RouterLink class="rail__link" to="/" :aria-label="t('rail.home')">
      <HLogo size="sm" decorative />
    </RouterLink>
    <!-- Aucun serveur enregistré : la liste des serveurs viendra ici. -->
    <HButton
      variant="ghost"
      disabled
      :hint="t('welcome.addServerSoon')"
      :aria-label="t('rail.addServer')"
    >
      <HIcon name="plus" />
    </HButton>
    <span class="rail__spacer" />
    <RouterLink class="rail__link rail__link--tool" to="/settings" :aria-label="t('rail.settings')">
      <HIcon name="settings" />
    </RouterLink>
  </nav>
</template>

<style scoped>
.rail {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-3);
  width: var(--rail-width);
  padding: var(--space-4) 0;
  border-right: 1px solid var(--bd);
  background: var(--side);
}

.rail__link {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 40px;
  min-height: 40px;
  border-radius: var(--radius-control);
}

.rail__link--tool {
  color: var(--tx2);
}

.rail__link--tool:hover,
.rail__link--tool.router-link-active {
  background: var(--card-2);
  color: var(--tx);
}

.rail__spacer {
  flex: 1;
}
</style>
