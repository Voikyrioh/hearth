import { defineStore } from "pinia";
import { reactive } from "vue";
import { type Draft, emptyDraft, isFilled, wipeDraft } from "@/deploy/draft";
import { useServersStore } from "./servers";

/**
 * Les brouillons du formulaire « Déployer un service », un par serveur (HRT-69, BR-DEPLOY-029). MÉMOIRE SEULEMENT :
 * ce magasin n'a ni persistance, ni écriture dans l'adresse, le stockage local ou un instantané. Les valeurs de
 * variables et la commande sont des secrets : elles vivent ici jusqu'à ce que l'issue du déploiement soit connue
 * (un échec rouvre le formulaire rempli), puis `wipe` les efface ; à l'abandon, aussitôt. Un serveur retiré du carnet
 * est effacé aussi. Aucune valeur n'est lue ailleurs que par le formulaire et par la demande qui part.
 */
export const useDeployFormStore = defineStore("deployForm", () => {
  const drafts = reactive(new Map<string, Draft>());
  let nextRow = 1;

  /** Le brouillon de ce serveur, créé vide s'il n'existe pas. */
  function of(serverId: string): Draft {
    let draft = drafts.get(serverId);
    if (!draft) {
      draft = emptyDraft();
      drafts.set(serverId, draft);
    }
    // `reactive` renvoie le proxy de la carte : on relit pour rendre le brouillon réactif.
    return drafts.get(serverId) as Draft;
  }

  /** Une identité neuve pour une rangée (clé de rendu stable, jamais réutilisée). */
  function rowId(): number {
    nextRow += 1;
    return nextRow;
  }

  function filled(serverId: string): boolean {
    const draft = drafts.get(serverId);
    return draft !== undefined && isFilled(draft);
  }

  /** Efface le brouillon de ce serveur de la mémoire (lancement, abandon). */
  function wipe(serverId: string): void {
    const draft = drafts.get(serverId);
    if (draft) wipeDraft(draft);
    drafts.delete(serverId);
  }

  function wipeAll(): void {
    for (const id of [...drafts.keys()]) wipe(id);
  }

  // Un serveur retiré du carnet est oublié.
  const servers = useServersStore();
  servers.$subscribe(() => {
    if (!servers.loaded) return;
    const ids = servers.servers.map((server) => server.id);
    for (const id of [...drafts.keys()]) if (!ids.includes(id)) wipe(id);
  });

  return { of, rowId, filled, wipe, wipeAll, has: (serverId: string) => drafts.has(serverId) };
});
