import { ref } from "vue";
import { reportUiError } from "@/errors/report";
import { t } from "@/i18n";
import { failureMessage, failureOf, getLinkBridge, type ServerInfo } from "@/link";
import { useToastsStore } from "@/stores/toasts";

/**
 * Le carnet de serveurs : modifier (quel serveur est en cours d'édition), supprimer (avec
 * confirmation), se déconnecter, oublier les identifiants mémorisés. Un échec devient une
 * notification claire (jamais une fenêtre bloquante) ; un incident inattendu est journalisé.
 * Toute la logique est ici, `Servers.vue` affiche.
 */
export function useServerBook() {
  const toasts = useToastsStore();
  const editing = ref<string | null>(null);
  const removing = ref<ServerInfo | null>(null);

  function notify(error: unknown, source: string) {
    const failure = failureOf(error);
    if (failure) toasts.push({ kind: "error", message: failureMessage(failure) });
    else reportUiError(error, source);
  }

  /** Déconnexion : le mot de passe mémorisé reste (BR-CONN-016). */
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

  /** Suppression confirmée : les identifiants mémorisés partent avec le serveur (BR-CONN-010). */
  async function confirmRemove() {
    const server = removing.value;
    removing.value = null;
    if (!server) return;
    try {
      await getLinkBridge().removeServer(server.id);
    } catch (error) {
      notify(error, "servers:remove");
    }
  }

  return { editing, removing, disconnect, forget, confirmRemove };
}
