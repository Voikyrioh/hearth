import { defineStore } from "pinia";
import { reactive } from "vue";
import { type FicheDraft, newFicheDraft, wipeFicheDraft } from "@/fiche/draft";
import type { FicheDetail } from "@/link";
import { useServersStore } from "./servers";

/**
 * Les brouillons de l'installation d'un serveur de jeu, un par serveur et par fiche (HRT-102, BR-FICHE-034). MÉMOIRE
 * SEULEMENT : ce magasin n'a ni persistance, ni écriture dans l'adresse, le stockage local ou un instantané. Une réponse
 * peut être un secret (mot de passe du serveur, jeton) : elle vit ici jusqu'à ce que l'issue de l'installation soit
 * connue (un échec rouvre le formulaire rempli), puis `wipe` l'efface ; à l'abandon, aussitôt. Un serveur retiré du
 * carnet est effacé aussi. Une fiche qui a changé depuis le début de la saisie (version ou empreinte) repart d'un
 * brouillon neuf : on n'installe jamais une fiche que l'utilisateur n'a pas lue.
 */
export const useFicheInstallStore = defineStore("ficheInstall", () => {
  const drafts = reactive(new Map<string, FicheDraft>());

  const keyOf = (serverId: string, ficheId: string) => `${serverId}/${ficheId}`;

  /** Le brouillon de cette fiche sur ce serveur, s'il existe. */
  function of(serverId: string, ficheId: string): FicheDraft | undefined {
    return drafts.get(keyOf(serverId, ficheId));
  }

  /**
   * Le brouillon de cette fiche telle que l'utilisateur vient de la lire : celui qui existe s'il porte la même version
   * et la même empreinte, sinon un brouillon neuf (l'ancien est effacé).
   */
  function begin(serverId: string, detail: FicheDetail, name: () => string): FicheDraft {
    const key = keyOf(serverId, detail.card.id);
    const known = drafts.get(key);
    if (known && known.version === detail.card.version && known.digest === detail.digest) {
      return known;
    }
    if (known) wipeFicheDraft(known);
    drafts.set(key, newFicheDraft(detail, name()));
    return drafts.get(key) as FicheDraft;
  }

  /** Une saisie est en cours (fermer demande « Abandonnez-vous votre saisie ? »). */
  function edited(serverId: string, ficheId: string): boolean {
    return drafts.get(keyOf(serverId, ficheId))?.edited === true;
  }

  /** Efface le brouillon de la mémoire (lancement, abandon). */
  function wipe(serverId: string, ficheId: string): void {
    const key = keyOf(serverId, ficheId);
    const draft = drafts.get(key);
    if (draft) wipeFicheDraft(draft);
    drafts.delete(key);
  }

  function wipeAll(): void {
    for (const key of [...drafts.keys()]) {
      const draft = drafts.get(key);
      if (draft) wipeFicheDraft(draft);
      drafts.delete(key);
    }
  }

  // Un serveur retiré du carnet est oublié.
  const servers = useServersStore();
  servers.$subscribe(() => {
    if (!servers.loaded) return;
    const ids = servers.servers.map((server) => server.id);
    for (const key of [...drafts.keys()]) {
      if (!ids.includes(key.slice(0, key.indexOf("/")))) {
        const draft = drafts.get(key);
        if (draft) wipeFicheDraft(draft);
        drafts.delete(key);
      }
    }
  });

  return {
    of,
    begin,
    edited,
    wipe,
    wipeAll,
    has: (serverId: string, ficheId: string) => drafts.has(keyOf(serverId, ficheId)),
  };
});
