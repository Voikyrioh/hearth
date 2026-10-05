<script setup lang="ts">
import { ref } from "vue";
import { useRouter } from "vue-router";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import ConfirmDialog from "@/components/molecules/ConfirmDialog.vue";
import ServerEditForm from "@/components/organisms/ServerEditForm.vue";
import ServerRow from "@/components/organisms/ServerRow.vue";
import { reportUiError } from "@/errors/report";
import { t } from "@/i18n";
import { failureMessage, failureOf, getLinkBridge, type ServerInfo } from "@/link";
import { useServersStore } from "@/stores/servers";
import { useToastsStore } from "@/stores/toasts";

// Carnet de serveurs : tous les serveurs enregistrés, chacun avec son état de lien. Modifier
// (une autre adresse demande une nouvelle vérification de l'empreinte, BR-CONN-009), supprimer
// (ses identifiants mémorisés sont effacés avec, BR-CONN-010), se déconnecter (le mot de passe
// mémorisé reste, BR-CONN-016) ou oublier les identifiants mémorisés.
const servers = useServersStore();
const toasts = useToastsStore();
const router = useRouter();
const editing = ref<string | null>(null);
const removing = ref<ServerInfo | null>(null);

function notify(error: unknown, source: string) {
  const failure = failureOf(error);
  if (failure) toasts.push({ kind: "error", message: failureMessage(failure) });
  else reportUiError(error, source);
}

async function disconnect(server: ServerInfo) {
  try {
    await getLinkBridge().logout(server.id);
  } catch (error) {
    notify(error, "servers:logout");
  }
}

async function forget(server: ServerInfo) {
  try {
    await getLinkBridge().forgetCredentials(server.id);
    toasts.push({ kind: "success", message: t("connect.forgotten", { name: server.name }) });
  } catch (error) {
    notify(error, "servers:forget");
  }
}

async function remove() {
  const server = removing.value;
  removing.value = null;
  if (!server) return;
  try {
    await getLinkBridge().removeServer(server.id);
  } catch (error) {
    notify(error, "servers:remove");
  }
}
</script>

<template>
  <main class="book">
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
          v-if="editing === server.id"
          :server="server"
          @cancel="editing = null"
          @done="editing = null"
        />
        <ServerRow
          v-else
          :server="server"
          @edit="editing = server.id"
          @remove="removing = server"
          @disconnect="disconnect(server)"
          @forget="forget(server)"
        />
      </template>
    </ul>
    <ConfirmDialog
      :open="removing !== null"
      :title="t('connect.removeTitle')"
      :message="t('connect.removeText')"
      :confirm-label="t('connect.removeConfirm')"
      :cancel-label="t('connect.removeCancel')"
      destructive
      @confirm="remove"
      @cancel="removing = null"
    />
  </main>
</template>

<style scoped>
.book {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
  max-width: var(--panel-max);
  height: 100%;
  margin: 0 auto;
  padding: var(--page-pad);
  overflow-y: auto;
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
