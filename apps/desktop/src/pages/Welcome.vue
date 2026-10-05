<script setup lang="ts">
import { useRouter } from "vue-router";
import HButton from "@/components/atoms/HButton.vue";
import HLogo from "@/components/atoms/HLogo.vue";
import EmptyState from "@/components/molecules/EmptyState.vue";
import { t } from "@/i18n";

const router = useRouter();
</script>

<template>
  <main class="welcome">
    <EmptyState :title="t('welcome.title')" :text="t('welcome.text')">
      <template #illustration>
        <HLogo size="lg" decorative />
      </template>
      <template #action>
        <HButton @click="router.push({ name: 'add-server' })">{{ t("welcome.addServer") }}</HButton>
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
