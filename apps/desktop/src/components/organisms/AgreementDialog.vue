<script setup lang="ts">
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import FormDialog from "@/components/molecules/FormDialog.vue";
import { t } from "@/i18n";
import type { FicheDetail } from "@/link";

// La fenêtre du contrat de licence d'une fiche (HRT-102, BR-FICHE-058, BR-FICHE-026, design 18.3). Hearth n'accepte
// JAMAIS un contrat à la place de l'utilisateur : la fenêtre s'ouvre à « Installer le serveur de jeu », avant la
// confirmation du mot de passe, même après « Installer avec les valeurs proposées ». Titre : celui du contrat. Corps : la
// phrase de la fiche, « Lisez le contrat avant de répondre. », puis « Lire le contrat » (le navigateur de l'utilisateur ;
// Hearth ne charge jamais la page et n'affiche jamais son texte). Pied : « Je refuse » et « J'accepte », deux boutons
// secondaires de même poids, aucun en couleur d'accent, aucun avec le focus à l'ouverture (le focus est sur « Lire le
// contrat »). Échap et la fermeture valent « Je refuse ». Sans Internet, le bouton ouvre quand même le navigateur.
defineProps<{
  open: boolean;
  agreement: FicheDetail["agreements"][number];
  /** Le navigateur n'a pas pu être ouvert. */
  openFailed?: boolean;
}>();

const emit = defineEmits<{ accept: []; refuse: []; read: [] }>();
</script>

<template>
  <FormDialog
    :open="open"
    :title="safeText(agreement.title)"
    hide-submit
    data-agreement-dialog
    @cancel="emit('refuse')"
  >
    <p class="agree__sentence" data-agreement-sentence>{{ safeText(agreement.sentence) }}</p>
    <p class="agree__lead">{{ t("fiche.agreementLead") }}</p>
    <HButton variant="secondary" autofocus data-agreement-read @click="emit('read')">
      <HIcon name="external" size="sm" />
      {{ t("fiche.agreementRead") }}
    </HButton>
    <p v-if="openFailed" class="agree__failed" role="alert" data-agreement-failed>
      {{ t("fiche.linkOpenFailed") }}
    </p>
    <template #actions>
      <HButton variant="secondary" class="agree__button" data-agreement-refuse @click="emit('refuse')">
        {{ t("fiche.agreementRefuse") }}
      </HButton>
      <HButton variant="secondary" class="agree__button" data-agreement-accept @click="emit('accept')">
        {{ t("fiche.agreementAccept") }}
      </HButton>
    </template>
  </FormDialog>
</template>

<style scoped>
.agree__sentence,
.agree__lead {
  margin: 0;
}

.agree__failed {
  margin: 0;
  color: var(--crit);
}

.agree__button {
  flex: 1 1 0;
}
</style>
