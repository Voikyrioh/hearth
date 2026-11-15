<script setup lang="ts">
import { RouterLink } from "vue-router";
import HButton from "@/components/atoms/HButton.vue";
import Callout from "@/components/molecules/Callout.vue";
import DashCard from "@/components/molecules/DashCard.vue";
import ServiceRightReason from "@/components/molecules/ServiceRightReason.vue";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { usePageData } from "@/composables/usePageData";
import { useServiceRights } from "@/composables/useServiceRights";
import { t } from "@/i18n";

// L'aide « J'ai déjà un serveur de jeu » (HRT-106, BR-MONDE-021) : un parcours en trois temps pour amener un
// serveur de jeu qui tourne déjà dans Hearth sans risquer sa partie. Une page de lecture, ouverte à tous les profils : elle ne
// nomme AUCUN jeu ni aucun dossier (ce sont ceux du serveur de la personne) et n'a aucune adresse. Essayer en premier
// (un autre port, une copie du monde), basculer ensuite, et adopter seulement si l'on veut.
const { server } = useCurrentServer();
const rights = useServiceRights();
usePageData(() => true);

const allowed = () => rights.gate.value.kind === "allowed";
</script>

<template>
  <div class="help" data-existing-help>
    <p class="help__intro" data-help-intro>{{ t("fiche.help.intro") }}</p>

    <ol class="help__steps" :aria-label="t('fiche.help.stepsLabel')">
      <li class="help__step" data-help-step="try">
        <DashCard :title="t('fiche.help.tryTitle')">
          <p class="help__text">{{ t("fiche.help.tryText") }}</p>
          <RouterLink
            v-if="server"
            class="help__link"
            :to="{ name: 'catalogue', params: { id: server.id } }"
            data-help-catalogue
          >
            {{ t("fiche.help.tryLink") }}
          </RouterLink>
        </DashCard>
      </li>
      <li class="help__step" data-help-step="switch">
        <DashCard :title="t('fiche.help.switchTitle')">
          <p class="help__text">{{ t("fiche.help.switchText") }}</p>
        </DashCard>
      </li>
      <li class="help__step" data-help-step="adopt">
        <DashCard :title="t('fiche.help.adoptTitle')">
          <p class="help__text">{{ t("fiche.help.adoptText") }}</p>
          <RouterLink
            v-if="server && allowed()"
            :to="{ name: 'services', params: { id: server.id } }"
            custom
            v-slot="{ navigate }"
          >
            <HButton variant="secondary" data-help-adopt @click="navigate()">
              {{ t("fiche.help.adoptButton") }}
            </HButton>
          </RouterLink>
          <template v-else>
            <HButton
              variant="secondary"
              :disabled="true"
              :described-by="rights.reasonId"
              data-help-adopt
            >
              {{ t("fiche.help.adoptButton") }}
            </HButton>
            <ServiceRightReason :gate="rights.gate.value" :id="rights.reasonId" @retry="rights.retry()" />
          </template>
        </DashCard>
      </li>
    </ol>

    <Callout tone="info" data-help-advice>{{ t("fiche.help.advice") }}</Callout>

    <div class="help__pair">
      <DashCard :title="t('fiche.help.stayTitle')" data-help-stay>
        <p class="help__text">{{ t("fiche.help.stayText") }}</p>
      </DashCard>
      <DashCard :title="t('fiche.help.moveTitle')" data-help-move>
        <p class="help__text">{{ t("fiche.help.moveText") }}</p>
      </DashCard>
    </div>

    <Callout tone="info" data-help-never>
      <strong>{{ t("fiche.help.neverTitle") }}</strong>
      {{ t("fiche.help.neverText") }}
    </Callout>
    <Callout tone="info" data-help-folders>
      <strong>{{ t("fiche.help.foldersTitle") }}</strong>
      {{ t("fiche.help.foldersText") }}
    </Callout>

    <p class="help__foot" data-help-foot>{{ t("fiche.help.foot") }}</p>
  </div>
</template>

<style scoped>
.help {
  display: flex;
  flex-direction: column;
  gap: var(--card-gap);
  width: 100%;
  max-width: var(--column-max);
  min-width: 0;
}

.help__intro {
  margin: 0;
  color: var(--tx-secondary);
}

.help__steps {
  display: flex;
  flex-direction: column;
  gap: var(--card-gap);
  margin: 0;
  padding: 0;
  list-style: none;
}

.help__text {
  margin: 0;
}

.help__link {
  color: var(--ac);
  text-decoration: underline;
}

.help__pair {
  display: grid;
  grid-template-columns: 1fr;
  gap: var(--card-gap);
}

@media (min-width: 760px) {
  .help__pair {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

.help__foot {
  margin: 0;
  color: var(--tx-secondary);
}
</style>
