<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import Callout from "@/components/molecules/Callout.vue";
import DashCard from "@/components/molecules/DashCard.vue";
import { useCopyText } from "@/composables/useCopyText";
import { t } from "@/i18n";
import type { Address } from "@/services/game";

// La carte « Adresse pour vos joueurs » du détail d'un serveur de jeu (HRT-104, BR-JEU-020, BR-JEU-021) : l'adresse
// sous laquelle ce client joint le serveur, avec le port de base du jeu, et « Copier ». Visible de tous les profils.
// Service arrêté : le contenu est grisé et la phrase « personne ne peut se connecter » s'ajoute (hors du grisé, elle
// reste lisible). Lien coupé : l'adresse reste, « Copier » reste actif (c'est une copie locale). Hearth ne peut pas
// vérifier que l'adresse est joignable : la carte le dit. Aucune adresse publique n'est connue de ce client : la carte dit
// que celle-ci ne marche que depuis le réseau du serveur.
defineProps<{ address: Address | null; stopped: boolean }>();

const copier = useCopyText(() => t("game.address.toast"));
</script>

<template>
  <DashCard :title="t('game.address.title')" data-game-address>
    <p v-if="stopped" class="addr__stopped" role="status" data-game-address-stopped>
      <HIcon name="info" size="sm" />
      {{ t("game.address.stopped") }}
    </p>
    <div :class="['addr', { 'addr--off': stopped }]" data-game-address-body>
      <template v-if="address">
        <div class="addr__line">
          <code class="addr__value" :aria-label="t('game.address.label')" data-game-address-value>{{
            address.text
          }}</code>
          <HButton
            variant="secondary"
            size="sm"
            data-game-address-copy
            @click="copier.copy(address.text)"
          >
            <HIcon :name="copier.state.value === 'copied' ? 'check' : 'copy'" size="sm" />
            {{ copier.state.value === "copied" ? t("game.address.copied") : t("game.address.copy") }}
            <span class="sr-only">{{ t("game.address.copyLabel") }}</span>
          </HButton>
        </div>
        <p v-if="address.others" class="addr__note" data-game-address-others>
          {{ t("game.address.others") }}
        </p>
        <Callout v-if="copier.state.value === 'failed'" tone="warn" inline data-game-address-failed>
          {{ t("game.address.copyFailed") }}
        </Callout>
        <p class="addr__note" data-game-address-local>
          <HIcon name="info" size="sm" />
          {{ t("game.address.localOnly") }}
        </p>
        <p class="addr__note" data-game-address-unverified>
          <HIcon name="info" size="sm" />
          {{ t("game.address.unverified") }}
        </p>
      </template>
      <p v-else class="addr__note" data-game-address-none>{{ t("game.address.noPort") }}</p>
    </div>
  </DashCard>
</template>

<style scoped>
.addr {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.addr--off {
  opacity: var(--opacity-disabled);
}

.addr__line {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}

.addr__value {
  min-width: 0;
  color: var(--tx);
  font-family: var(--font-mono);
  font-size: var(--fs-h3);
  overflow-wrap: anywhere;
  user-select: text;
}

.addr__note,
.addr__stopped {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}

.addr__stopped {
  color: var(--tx);
}
</style>
