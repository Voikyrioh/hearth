import { defineStore } from "pinia";
import { ref } from "vue";
import { logUiError } from "@/errors/report";
import { failureOf, getLinkBridge } from "@/link";
import { useServiceRightsStore } from "./serviceRights";

/** Où en est la révélation : rien, lecture en cours, valeur montrée, ou échec (valeur toujours masquée). */
export type GamePasswordPhase = "idle" | "reading" | "shown" | "failed";
/** Pourquoi une lecture a échoué : le droit a été retiré (BR-JEU-025), ou simplement illisible. */
export type GamePasswordFailure = "lost" | "unreadable";

/**
 * Le mot de passe du serveur d'UN SEUL service de jeu à la fois (HRT-104, BR-JEU-025 à 027). Ce magasin n'est JAMAIS
 * persisté : rien dans le disque, rien dans un journal, rien dans le presse-papiers. La valeur n'existe qu'ici, en
 * mémoire, du moment où le compte la demande jusqu'au remasquage. `clear` l'EFFACE (la valeur est remplacée par `null`,
 * pas seulement cachée). Une réponse tardive d'un service qu'on a quitté entre-temps est jetée sans jamais être gardée.
 * Aucune valeur n'entre dans une erreur ni dans un rapport : seul un texte fixe est signalé.
 *
 * Le remasquage (fermeture du détail, autre service ou autre onglet, lien coupé, service disparu, droit retiré) est
 * demandé par l'écran à travers `clear` ; ce magasin ne le devine pas.
 */
export const useGamePasswordStore = defineStore("gamePassword", () => {
  const phase = ref<GamePasswordPhase>("idle");
  const failure = ref<GamePasswordFailure | null>(null);
  /**
   * La valeur révélée ; `null` : effacée. `""` : le serveur n'a pas de mot de passe (BR-JEU-027) : rien à montrer
   * que la phrase qui le dit.
   */
  const password = ref<string | null>(null);
  /** Le service dont la valeur est (ou va être) montrée. */
  const target = ref<{ serverId: string; key: string } | null>(null);
  let ticket = 0;

  /** Efface tout : la valeur n'est plus nulle part. */
  function clear(): void {
    ticket += 1;
    phase.value = "idle";
    failure.value = null;
    password.value = null;
    target.value = null;
  }

  /** Demande la valeur de ce service (une lecture consignée par l'agent). Sans effet si une lecture est en cours. */
  async function reveal(serverId: string, key: string): Promise<void> {
    if (phase.value === "reading") return;
    const mine = ++ticket;
    password.value = null;
    failure.value = null;
    target.value = { serverId, key };
    phase.value = "reading";
    try {
      const read = await getLinkBridge().revealGamePassword(serverId, key);
      // On a remasqué, changé de service ou quitté pendant la lecture : la réponse est jetée.
      if (mine !== ticket) return;
      password.value = read.password ?? "";
      phase.value = "shown";
    } catch (error) {
      if (mine !== ticket) return;
      password.value = null;
      phase.value = "failed";
      const kind = failureOf(error)?.kind;
      failure.value = kind === "missing_services_right" ? "lost" : "unreadable";
      if (kind === "missing_services_right") {
        // Le droit a été retiré entre-temps : on le relit (BR-GRANT-011) pour que les boutons suivent.
        void useServiceRightsStore().noteRefusal(serverId);
      } else if (!kind) {
        // Jamais le message d'origine : il pourrait citer ce qui revient du serveur.
        logUiError(new Error("lecture du mot de passe en échec"), "gamePassword:reveal");
      }
    }
  }

  return { phase, failure, password, target, reveal, clear };
});
