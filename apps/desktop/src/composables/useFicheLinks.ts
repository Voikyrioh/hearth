import { logUiError } from "@/errors/report";
import { t } from "@/i18n";
import { getLinkBridge } from "@/link";
import { useToastsStore } from "@/stores/toasts";

/**
 * Ouvrir dans le navigateur de l'utilisateur une adresse que la fiche porte (HRT-102) : le contrat de licence, le site où
 * créer un jeton de serveur, la source d'un chiffre de l'éditeur. L'interface ne donne jamais l'adresse : elle nomme le genre
 * et l'identifiant, la coquille relit la fiche chez l'agent. UN seul endroit pour la fiche, l'installation et la fenêtre
 * du récapitulatif. `open` rend `false` si le navigateur n'a pas pu s'ouvrir ; `notify` (par défaut) le dit alors par une
 * notification, sauf sous une fenêtre modale (la notification passerait dessous) : l'appelant le dit alors dans sa fenêtre.
 */
export function useFicheLinks(serverId: () => string, ficheId: () => string) {
  const toasts = useToastsStore();

  async function open(
    kind: "agreement" | "token" | "evidence",
    key: string,
    options: { notify?: boolean } = {},
  ): Promise<boolean> {
    try {
      await getLinkBridge().openFicheLink(serverId(), ficheId(), kind, key);
      return true;
    } catch (error) {
      logUiError(error, "fiche:link");
      if (options.notify !== false) {
        toasts.push({ kind: "error", message: t("fiche.agreementOpenFailed") });
      }
      return false;
    }
  }

  return { open };
}
