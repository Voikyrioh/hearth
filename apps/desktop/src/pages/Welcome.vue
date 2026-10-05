<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import HLogo from "@/components/atoms/HLogo.vue";
import EmptyState from "@/components/molecules/EmptyState.vue";
import { t } from "@/i18n";
</script>

<template>
  <main class="welcome">
    <EmptyState :title="t('welcome.title')" :text="t('welcome.text')">
      <template #illustration>
        <HLogo size="lg" decorative />
      </template>
      <template #action>
        <!-- Inactif tant que l'assistant d'ajout de serveur n'existe pas. -->
        <HButton disabled :hint="t('welcome.addServerSoon')">{{ t("welcome.addServer") }}</HButton>
      </template>
    </EmptyState>
  </main>
</template>

<style scoped>
.welcome {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  padding: var(--space-5);
}
</style>
