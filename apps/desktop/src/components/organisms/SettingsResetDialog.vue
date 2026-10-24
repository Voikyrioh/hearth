<script setup lang="ts">
import { safeText } from "@/audit/text";
import FormDialog from "@/components/molecules/FormDialog.vue";
import { t } from "@/i18n";

// La confirmation d'une remise en bloc (HRT-109, BR-FICHE-044) : « Remettre ce thème aux valeurs proposées » ou « Tout
// remettre aux valeurs proposées ». Elle liste les réglages concernés avec leur valeur actuelle et leur valeur proposée ;
// elle dit que ni les réglages figés ou imposés, ni les mots de passe ne sont touchés, et que la remise ne prend effet
// qu'à l'enregistrement. Un réglage sans valeur proposée est remis en retirant sa ligne du fichier : la fenêtre le dit.
defineProps<{
  open: boolean;
  title: string;
  /** Les réglages concernés : libellé de la fiche, valeur actuelle et valeur proposée déjà en phrase (`null` : pas de proposée). */
  items: readonly { id: string; label: string; current: string; proposed: string | null }[];
}>();

const emit = defineEmits<{ confirm: []; cancel: [] }>();
</script>

<template>
  <FormDialog
    :open="open"
    :title="title"
    :submit-label="t('gameSettings.resetConfirm')"
    :hide-submit="items.length === 0"
    wide
    data-reset-dialog
    @submit="emit('confirm')"
    @cancel="emit('cancel')"
  >
    <p v-if="items.length === 0" data-reset-nothing>{{ t("gameSettings.resetNothing") }}</p>
    <template v-else>
      <p class="reset__note">{{ t("gameSettings.resetNote") }}</p>
      <ul class="reset__list" data-reset-list>
        <li v-for="item in items" :key="item.id" :data-reset-item="item.id">
          {{
            item.proposed === null
              ? t("gameSettings.resetLineRemoved", { label: safeText(item.label), current: item.current })
              : t("gameSettings.resetLine", {
                  label: safeText(item.label),
                  current: item.current,
                  proposed: item.proposed,
                })
          }}
          <span v-if="item.proposed === null" class="reset__why">{{ t("gameSettings.resetNoProposalWhy") }}</span>
        </li>
      </ul>
    </template>
  </FormDialog>
</template>

<style scoped>
.reset__note {
  margin: 0 0 var(--space-3);
  color: var(--tx-secondary);
}

.reset__list {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin: 0;
  padding-left: var(--space-5);
}

.reset__why {
  display: block;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}
</style>
