<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import ComingSoonPanel from "@/components/molecules/ComingSoonPanel.vue";
import { t } from "@/i18n";
import { useToastsStore } from "@/stores/toasts";

// Comptes : « Bientôt disponible » (son ticket suit). L'action de l'en-tête exige le lien.
const toasts = useToastsStore();
</script>

<template>
  <Teleport defer to="#header-actions">
    <HButton needs-link tip-placement="end" @click="toasts.push({ kind: 'info', message: t('common.comingSoon') })">
      {{ t("pages.addAccount") }}
    </HButton>
  </Teleport>
  <ComingSoonPanel />
</template>
