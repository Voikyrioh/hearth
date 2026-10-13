import { defineStore } from "pinia";
import { shallowReactive, watch } from "vue";
import { logUiError } from "@/errors/report";
import { failureOf, getLinkBridge } from "@/link";
import type { RightsState } from "@/services/rights";
import { useLinkStore } from "./link";
import { useServersStore } from "./servers";

/** Au plus ce délai pour lire le droit : au-delà, la lecture est dite en échec (BR-GRANT-011, états UI). */
export const RIGHTS_READ_LIMIT_MS = 5000;

/**
 * Le droit « gérer les services » du compte connecté, par serveur (HRT-78, BR-GRANT-011). Lu à l'ouverture de la
 * section Services, à chaque reconnexion et après tout refus pour absence du droit (`noteRefusal`) ; jamais écrit
 * (BR-GRANT-005). Entre deux lectures un droit retiré au serveur peut rester affiché jusqu'au premier refus : le
 * serveur fait foi (BR-GRANT-004).
 *
 * Lien coupé : les derniers droits connus sont conservés et la lecture n'est PAS dite en échec (les boutons sont
 * alors désactivés pour cause de lien coupé, pas pour le droit). Une lecture plus ancienne qu'une autre n'écrase
 * jamais la plus récente.
 *
 * Le droit est celui d'UN compte (C3 de la review) : l'entrée garde le compte pour lequel elle a été lue (`owners`) et
 * `of` ne la rend que tant que le serveur est connecté avec ce compte, donc le compte B ne voit jamais, même un
 * instant, le droit ni l'identifiant du compte A. Session fermée ou expirée, accès révoqué, autre compte, serveur
 * retiré : l'entrée est oubliée (`forget`).
 */
export const useServiceRightsStore = defineStore("serviceRights", () => {
  const entries = shallowReactive(new Map<string, RightsState>());
  const tickets = new Map<string, number>();
  /** Le compte connecté au moment de la lecture, par serveur. */
  const owners = new Map<string, string>();
  const servers = useServersStore();
  const link = useLinkStore();
  /** Numéro de lecture global : jamais réutilisé, même après un `forget` (une lecture tardive d'un autre compte est écartée). */
  let lastTicket = 0;

  const currentAccount = (serverId: string) => servers.byId(serverId)?.username ?? "";

  function of(serverId: string): RightsState | undefined {
    const entry = entries.get(serverId);
    if (!entry) return undefined;
    return owners.get(serverId) === currentAccount(serverId) ? entry : undefined;
  }

  /** Lit (ou relit) le droit du compte connecté sur ce serveur. */
  async function load(serverId: string): Promise<void> {
    const ticket = ++lastTicket;
    tickets.set(serverId, ticket);
    const account = currentAccount(serverId);
    // Un autre compte que celui de l'entrée gardée : on repart de zéro, rien de l'ancien n'est conservé.
    if (owners.get(serverId) !== account) entries.delete(serverId);
    owners.set(serverId, account);
    const known = entries.get(serverId);
    // Une première lecture, ou une lecture après échec : « en cours ». Une relecture garde le dernier droit connu.
    if (!known || known.phase === "failed") {
      entries.set(serverId, { phase: "reading", rights: known?.rights ?? null });
    }
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      const limit = new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error("lecture du droit trop longue")),
          RIGHTS_READ_LIMIT_MS,
        );
      });
      const rights = await Promise.race([getLinkBridge().readServiceRights(serverId), limit]);
      if (tickets.get(serverId) !== ticket) return;
      entries.set(serverId, { phase: "known", rights });
    } catch (error) {
      if (tickets.get(serverId) !== ticket) return;
      const current = entries.get(serverId);
      // Lien coupé (rien n'est parti) : les derniers droits connus restent, ce n'est pas un échec de lecture.
      if (failureOf(error)?.kind === "not_connected" && current?.rights) {
        entries.set(serverId, { phase: "known", rights: current.rights });
        return;
      }
      entries.set(serverId, { phase: "failed", rights: current?.rights ?? null });
      if (!failureOf(error)) logUiError(error, "serviceRights:load");
    } finally {
      clearTimeout(timer);
    }
  }

  /**
   * Le serveur a refusé un acte faute du droit (`missing_services_right`) : le droit affiché était périmé, il est
   * relu tout de suite (BR-GRANT-011) et les boutons se désactivent avec la raison.
   */
  function noteRefusal(serverId: string): Promise<void> {
    return load(serverId);
  }

  function forget(serverId: string): void {
    entries.delete(serverId);
    tickets.delete(serverId);
    owners.delete(serverId);
  }

  // La session change (fermée, expirée, accès révoqué), le compte change ou le serveur disparaît : le droit lu
  // n'est plus celui de la session en cours.
  watch(
    () =>
      [
        servers.servers.map((server) => `${server.id}|${server.username}`).join(";"),
        [...entries.keys()].map((id) => `${id}|${link.stateOf(id) ?? ""}`).join(";"),
      ] as const,
    () => {
      for (const serverId of [...entries.keys()]) {
        const state = link.stateOf(serverId);
        if (
          (servers.loaded && !servers.byId(serverId)) ||
          owners.get(serverId) !== currentAccount(serverId) ||
          state === "session_expired" ||
          state === "access_revoked"
        ) {
          forget(serverId);
        }
      }
    },
    { flush: "sync" },
  );

  /** Oublie tout (tests). */
  function reset(): void {
    entries.clear();
    tickets.clear();
    owners.clear();
  }

  return { entries, of, load, noteRefusal, forget, reset };
});
