import { ref } from "vue";
import { t } from "@/i18n";
import { failureMessage, failureOf, getLinkBridge, type ServerInfo } from "@/link";
import { useCountdown } from "./useCountdown";

/**
 * Reconnexion d'un serveur enregistré sans session (session expirée, déconnexion, mot de passe
 * mémorisé refusé). L'état (erreur, compte à rebours, tentative en cours) est celui d'UN serveur :
 * le composant qui l'utilise est recréé quand on change de serveur (`:key`), la saisie de l'un ne
 * part jamais vers l'autre. Le mot de passe refusé n'est pas gardé.
 */
export function useReconnect(server: () => ServerInfo) {
  const countdown = useCountdown();
  const busy = ref(false);
  const error = ref<string | null>(null);
  /** Le serveur pour lequel la tentative a été lancée (aucune réponse ne vaut pour un autre). */
  const target = server().id;

  /** Rend vrai si la connexion a réussi. */
  async function submit(entry: {
    username: string;
    password: string;
    remember: boolean;
  }): Promise<boolean> {
    if (busy.value || countdown.active.value) return false;
    error.value = null;
    busy.value = true;
    try {
      await getLinkBridge().login(target, entry.username, entry.password, entry.remember);
      return true;
    } catch (failure) {
      const reason = failureOf(failure);
      if (reason?.kind === "too_many_attempts") countdown.start(reason.retry_after_s);
      else error.value = reason ? failureMessage(reason) : t("failure.generic");
      return false;
    } finally {
      busy.value = false;
    }
  }

  return { busy, error, lockedSeconds: countdown.remaining, submit };
}
