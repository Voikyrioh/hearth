<script setup lang="ts">
import { computed } from "vue";
import { t } from "@/i18n";
import { grantCommand } from "@/services/rights";
import CommandBlock from "./CommandBlock.vue";

// La ligne « Gestion des services » du détail d'un compte (HRT-78, BR-GRANT-013, écran I du design) : la valeur
// (« Autorisée » ou « Non autorisée »), l'aide, et la commande à lancer sur le serveur avec « Copier la commande ».
// EN LECTURE SEULE : ni case ni commutateur, ce serait promettre un geste que le client n'a pas (BR-GRANT-005). La
// commande n'est construite que pour un identifiant qui passe `isHearthUsername` (même validation que l'aide du
// moteur) : sinon aucune commande, et le nom n'est pas montré. Agent ancien : la valeur n'est pas « Non autorisée »
// (il ne sait pas le dire), la ligne dit de mettre le serveur à jour (BR-GRANT-016).
const props = defineProps<{
  /** L'identifiant du compte (texte de l'agent, validé avant d'entrer dans une commande). */
  username: string;
  manages: boolean;
  /** L'agent annonce `service-rights`. */
  known: boolean;
  /** C'est le compte de la personne qui regarde : la commande qui retire le droit est précédée d'un avertissement. */
  own?: boolean;
}>();

const command = computed(() => (props.known ? grantCommand(props.username, props.manages) : null));
</script>

<template>
  <div class="row" data-rights-row>
    <dl class="row__line">
      <dt>{{ t("rights.rowLabel") }}</dt>
      <dd data-rights-value>
        {{ !known ? t("rights.oldAgent") : manages ? t("rights.allowed") : t("rights.notAllowed") }}
      </dd>
    </dl>
    <template v-if="known">
      <p class="row__help" data-rights-permits>{{ t("rights.permits") }}</p>
      <p class="row__help" data-rights-help>{{ t("rights.help") }}</p>
      <p v-if="own && manages && command" class="row__warn" data-rights-own-warning>
        {{ t("rights.revokeOwnWarning") }}
      </p>
      <CommandBlock
        v-if="command"
        :text="command"
        :label="t('rights.commandLabel')"
        :copy-label="t('rights.copyCommand')"
        :copied-label="t('rights.copied')"
        :failed-label="t('rights.copyFailed')"
        data-rights-command
      />
    </template>
  </div>
</template>

<style scoped>
.row {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
}

.row__line {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--space-4);
  margin: 0;
}

.row__line dt {
  color: var(--tx2);
}

.row__line dd {
  margin: 0;
  font-weight: var(--fw-semibold);
  text-align: right;
}

.row__help {
  margin: 0;
  color: var(--tx2);
  font-size: var(--fs-small);
}

.row__warn {
  margin: 0;
  color: var(--warn);
  font-size: var(--fs-small);
}
</style>
