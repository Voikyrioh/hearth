import { computed } from "vue";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { t } from "@/i18n";

/**
 * La phrase d'une carte qui n'a jamais rien lu parce que le lien manque (HRT-38, C46), ou `null` quand le lien est
 * « Connecté » (l'échec est alors un vrai échec, avec son « Réessayer »). La cause est dite avec justesse : le serveur
 * qui ne répond pas « reviendra » ; en session expirée ou accès révoqué le serveur répond, c'est la session qui manque.
 */
export function useNotLoadedText() {
  const { state } = useCurrentServer();
  return computed(() => {
    switch (state.value) {
      case "offline":
      case "reconnecting":
        return t("link.notLoadedYet");
      case "session_expired":
      case "access_revoked":
        return t("link.notLoadedSession");
      default:
        return null;
    }
  });
}
