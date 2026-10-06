import { defineStore } from "pinia";
import { ref } from "vue";
import { reportUiError } from "@/errors/report";
import { type Account, type AccountRefusal, failureOf, getLinkBridge } from "@/link";

/**
 * La liste des comptes de chaque serveur, telle que l'agent l'a rendue (jamais un mot de passe ni un
 * haché ; rien n'est stocké hors de la mémoire de la page). Une liste déjà lue reste affichée pendant
 * qu'on la relit (aucun clignotement), et si la relecture échoue : ni vidée ni fausse, juste
 * `error`. Une lecture plus ancienne qu'une autre ne l'écrase jamais.
 */
export type AccountsStatus = "loading" | "ready" | "refused" | "error";

export interface ServerAccounts {
  status: AccountsStatus;
  accounts: Account[];
  refusal: AccountRefusal | null;
}

export const useAccountsStore = defineStore("accounts", () => {
  const byServer = ref<Record<string, ServerAccounts>>({});
  const reads = new Map<string, number>();

  function of(serverId: string): ServerAccounts | undefined {
    return byServer.value[serverId];
  }

  function put(serverId: string, next: ServerAccounts) {
    byServer.value = { ...byServer.value, [serverId]: next };
  }

  /** Lit (ou relit) la liste ; une réponse plus ancienne que la dernière demandée est écartée. */
  async function load(serverId: string): Promise<void> {
    const ticket = (reads.get(serverId) ?? 0) + 1;
    reads.set(serverId, ticket);
    const known = byServer.value[serverId];
    if (!known || known.status === "error" || known.status === "refused") {
      put(serverId, { status: "loading", accounts: known?.accounts ?? [], refusal: null });
    }
    try {
      const list = await getLinkBridge().listAccounts(serverId);
      if (reads.get(serverId) !== ticket) return;
      if (list.kind === "listed") {
        put(serverId, { status: "ready", accounts: list.accounts, refusal: null });
      } else {
        put(serverId, { status: "refused", accounts: [], refusal: list.refusal });
      }
    } catch (error) {
      if (reads.get(serverId) !== ticket) return;
      // Lien coupé ou serveur parti : la dernière liste connue reste (le gabarit la désature).
      const current = byServer.value[serverId];
      put(serverId, { status: "error", accounts: current?.accounts ?? [], refusal: null });
      if (!failureOf(error)) reportUiError(error, "accounts:load");
    }
  }

  function forget(serverId: string) {
    const { [serverId]: _gone, ...rest } = byServer.value;
    byServer.value = rest;
    reads.delete(serverId);
  }

  function reset() {
    byServer.value = {};
    reads.clear();
  }

  return { byServer, of, load, forget, reset };
});
