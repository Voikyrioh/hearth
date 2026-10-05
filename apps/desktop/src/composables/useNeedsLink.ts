import { type ComputedRef, computed } from "vue";
import { type MessageKey, t } from "@/i18n";
import type { LinkState } from "@/link";
import { useLinkStore } from "@/stores/link";
import { useServersStore } from "@/stores/servers";

/** `needs-link` d'un composant : `true`, ou `{ role: 'admin' }` pour exiger aussi le rôle administrateur. */
export interface NeedsLinkOptions {
  role?: "admin";
}
export type NeedsLink = boolean | NeedsLinkOptions | undefined;

const LINK_REASONS: Record<Exclude<LinkState, "connected">, MessageKey> = {
  reconnecting: "needs.reconnecting",
  offline: "needs.offline",
  session_expired: "needs.sessionExpired",
  access_revoked: "needs.accessRevoked",
};

/**
 * LA façon d'exiger le serveur (BR-RESIL-008) : un seul calcul, une seule source de vérité.
 * Rend l'explication à montrer (infobulle) quand le lien du serveur courant n'est pas
 * « Connecté », ou que le rôle ne suffit pas, sinon `null`. `HButton`, `HToggle` et `HInput`
 * l'appellent pour leur prop `needsLink` et l'ajoutent à leur propre état (désactivé,
 * occupé) : la désactivation finale se calcule dans le composant, jamais en retouchant le DOM.
 */
export function useNeedsLink(source: () => NeedsLink): ComputedRef<string | null> {
  return computed(() => {
    const wanted = source();
    if (!wanted) return null;
    // Les stores ne sont touchés que si le composant exige le serveur.
    const servers = useServersStore();
    const link = useLinkStore();
    const server = servers.current;
    if (!server) return t("needs.noServer");
    if (typeof wanted === "object" && wanted.role === "admin" && server.role !== "admin") {
      return t("needs.role");
    }
    const state = link.stateOf(server.id);
    return state === "connected" ? null : t(LINK_REASONS[state]);
  });
}
