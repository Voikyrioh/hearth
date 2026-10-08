<script setup lang="ts">
import { useRouter } from "vue-router";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import ConfirmDialog from "@/components/molecules/ConfirmDialog.vue";
import ServerEditForm from "@/components/organisms/ServerEditForm.vue";
import ServerRow from "@/components/organisms/ServerRow.vue";
import { useServerBook } from "@/composables/useServerBook";
import { t } from "@/i18n";
import { useServersStore } from "@/stores/servers";

// Carnet de serveurs : tous les serveurs enregistrés, chacun avec son état de lien. Modifier
// (une autre adresse demande une nouvelle vérification de l'empreinte, BR-CONN-009), supprimer
// (ses identifiants mémorisés sont effacés avec, BR-CONN-010), se déconnecter (le mot de passe
// mémorisé reste, BR-CONN-016) ou oublier les identifiants mémorisés. La logique est dans
// `useServerBook` ; ici, l'affichage.
const servers = useServersStore();
const router = useRouter();
const book = useServerBook();
</script>

<template>
  <main class="book__scroll">
    <div class="book">
    <header class="book__head">
      <h1 class="book__title">{{ t("connect.booksTitle") }}</h1>
      <HButton @click="router.push({ name: 'add-server' })">
        <HIcon name="plus" size="sm" />
        {{ t("connect.add") }}
      </HButton>
    </header>
    <ul class="book__list">
      <template v-for="server in servers.servers" :key="server.id">
        <ServerEditForm
          v-if="book.editing.value === server.id"
          :server="server"
          @cancel="book.editing.value = null"
          @done="book.editing.value = null"
        />
        <ServerRow
          v-else
          :server="server"
          @edit="book.editing.value = server.id"
          @remove="book.removing.value = server"
          @disconnect="book.disconnect(server)"
          @forget="book.forget(server)"
        />
      </template>
    </ul>
    <ConfirmDialog
      :open="book.removing.value !== null"
      :title="t('connect.removeTitle')"
      :message="t('connect.removeText')"
      :confirm-label="t('connect.removeConfirm')"
      :cancel-label="t('connect.removeCancel')"
      destructive
      @confirm="book.confirmRemove()"
      @cancel="book.removing.value = null"
    />
    </div>
  </main>
</template>

<style scoped>
.book__scroll {
  height: 100%;
  overflow-y: auto;
}

.book {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
  max-width: var(--panel-max);
  margin: 0 auto;
  padding: var(--page-pad);
}

.book__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
}

.book__title {
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.book__list {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: 0;
  margin: 0;
}
</style>
