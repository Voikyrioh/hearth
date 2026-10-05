<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import ComingSoonPanel from "@/components/molecules/ComingSoonPanel.vue";
import { t } from "@/i18n";
import { useToastsStore } from "@/stores/toasts";

// Journal d'activité : « Bientôt disponible » (son ticket suit). L'export exige le lien.
const toasts = useToastsStore();
</script>

<template>
  <Teleport defer to="#header-actions">
    <HButton
      needs-link
      tip-placement="end"
      variant="secondary"
      @click="toasts.push({ kind: 'info', message: t('common.comingSoon') })"
    >
      {{ t("pages.export") }}
    </HButton>
  </Teleport>
  <ComingSoonPanel />
</template>
