<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import Callout from "@/components/molecules/Callout.vue";
import DashCard from "@/components/molecules/DashCard.vue";
import { useCopyText } from "@/composables/useCopyText";
import { t } from "@/i18n";
import type { Addresses } from "@/services/game";

// La carte « Adresse pour vos joueurs » du détail d'un serveur de jeu (HRT-104, BR-JEU-020, BR-JEU-021) : l'adresse
// sous laquelle ce client joint le serveur, avec le port de base du jeu, et « Copier ». Visible de tous les profils.
// HRT-223 : deux adresses, celle du réseau maison (lue par l'agent sur la machine) et celle d'internet (renseignée par
// le propriétaire, sinon dite par la box en UPnP, jamais par un service tiers). Service arrêté : le contenu est grisé et la phrase « personne ne peut se connecter » s'ajoute (hors du grisé, elle
// reste lisible). Lien coupé : l'adresse reste, « Copier » reste actif (c'est une copie locale). Hearth ne peut pas
// vérifier que l'adresse est joignable : la carte le dit. Aucune adresse publique n'est connue de ce client : la carte dit
// que celle-ci ne marche que depuis le réseau du serveur.
defineProps<{
  addresses: Addresses | null;
  stopped: boolean;
  /** Le compte peut changer l'adresse (droit « gérer les services », lien et moteur prêts). */
  canEdit: boolean;
  /** Pourquoi le bouton est inactif, écrit sous la carte ; `undefined` : il est actif ou absent. */
  reason?: string;
}>();
const emit = defineEmits<{ edit: [] }>();

const copier = useCopyText(() => t("game.address.toast"));
</script>

<template>
  <DashCard :title="t('game.address.title')" data-game-address>
    <template #aside>
      <HButton
        variant="secondary"
        size="sm"
        needs-link
        :disabled="!canEdit"
        data-game-address-edit
        @click="emit('edit')"
      >
        {{ t("game.address.edit") }}
      </HButton>
    </template>
    <p v-if="stopped" class="addr__stopped" role="status" data-game-address-stopped>
      <HIcon name="info" size="sm" />
      {{ t("game.address.stopped") }}
    </p>
    <div :class="['addr', { 'addr--off': stopped }]" data-game-address-body>
      <template v-if="addresses">
        <section class="addr__block" data-game-address-lan>
          <h4 class="addr__label">{{ t("game.address.lanTitle") }}</h4>
          <div class="addr__line">
            <code class="addr__value" :aria-label="t('game.address.lanLabel')" data-game-address-lan-value>{{
              addresses.lan
            }}</code>
            <HButton
              variant="secondary"
              size="sm"
              data-game-address-lan-copy
              @click="copier.copy(addresses.lan)"
            >
              <HIcon :name="copier.state.value === 'copied' ? 'check' : 'copy'" size="sm" />
              {{ copier.state.value === "copied" ? t("game.address.copied") : t("game.address.copy") }}
              <span class="sr-only">{{ t("game.address.copyLabel") }}</span>
            </HButton>
          </div>
          <p class="addr__note">{{ t("game.address.lanHelp") }}</p>
        </section>
        <section class="addr__block" data-game-address-internet>
          <h4 class="addr__label">{{ t("game.address.internetTitle") }}</h4>
          <template v-if="addresses.internet">
            <div class="addr__line">
              <code class="addr__value" :aria-label="t('game.address.internetLabel')" data-game-address-value>{{
                addresses.internet.text
              }}</code>
              <HButton
                variant="secondary"
                size="sm"
                data-game-address-copy
                @click="copier.copy(addresses.internet.text)"
              >
                <HIcon :name="copier.state.value === 'copied' ? 'check' : 'copy'" size="sm" />
                {{ copier.state.value === "copied" ? t("game.address.copied") : t("game.address.copy") }}
                <span class="sr-only">{{ t("game.address.copyLabel") }}</span>
              </HButton>
            </div>
            <p v-if="addresses.internet.source === 'entered'" class="addr__note" data-game-address-public>
              <HIcon name="info" size="sm" />
              {{ t("game.address.publicNote") }}
            </p>
            <p v-else class="addr__note" data-game-address-detected>
              <HIcon name="info" size="sm" />
              {{ t("game.address.detectedNote") }}
            </p>
            <p class="addr__note">{{ t("game.address.internetHelp") }}</p>
          </template>
          <p v-else class="addr__note" data-game-address-local>
            <HIcon name="info" size="sm" />
            {{ t("game.address.localOnly") }}
          </p>
        </section>
        <p v-if="addresses.others" class="addr__note" data-game-address-others>
          {{ t("game.address.others") }}
        </p>
        <Callout v-if="copier.state.value === 'failed'" tone="warn" inline data-game-address-failed>
          {{ t("game.address.copyFailed") }}
        </Callout>
        <p class="addr__note" data-game-address-unverified>
          <HIcon name="info" size="sm" />
          {{ t("game.address.unverified") }}
        </p>
      </template>
      <p v-else class="addr__note" data-game-address-none>{{ t("game.address.noPort") }}</p>
      <p v-if="reason" class="addr__note" data-game-address-reason>{{ reason }}</p>
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

.addr__block {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.addr__label {
  margin: 0;
  color: var(--tx);
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
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
