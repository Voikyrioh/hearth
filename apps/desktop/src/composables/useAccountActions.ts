import { refusalMessage, roleLabel } from "@/accounts/messages";
import { useServerAction } from "@/composables/useServerAction";
import { t } from "@/i18n";
import {
  type Account,
  type AccountOutcome,
  type AccountRefusal,
  getLinkBridge,
  type Role,
} from "@/link";
import { useAccountsStore } from "@/stores/accounts";
import { useToastsStore } from "@/stores/toasts";

/**
 * Ce que l'appelant (une boîte de dialogue ou une ligne) doit savoir d'une action de compte :
 * `done` (le succès est déjà annoncé et la liste relue), `refused` (à montrer où l'on est), `unknown`
 * (le lien est tombé avant la réponse : déjà dit, JAMAIS rejouée, la liste se relit au retour du
 * lien), `failed` (rien n'est parti ou la commande a échoué : déjà notifié).
 */
export type AccountReport =
  | { kind: "done"; sessionsClosed: number }
  | { kind: "refused"; refusal: AccountRefusal }
  | { kind: "unknown" }
  | { kind: "failed" };

/**
 * Les actions de comptes d'un serveur. Chacune passe par `useServerAction` (une seule à la fois,
 * résultat inconnu à la coupure, jamais rejouée) et par UNE commande typée du pont. Aucun mot de
 * passe n'est gardé ici : il traverse l'appel et c'est tout (l'appelant vide ses champs après
 * l'envoi, réussi ou non). Le contrôle d'accès est celui de l'agent : un refus `forbidden` est
 * rendu proprement.
 */
export function useAccountActions(serverId: () => string) {
  const action = useServerAction();
  const toasts = useToastsStore();
  const accounts = useAccountsStore();

  function success(message: string) {
    toasts.push({ kind: "success", message });
  }

  /**
   * Relit la liste d'un serveur SEULEMENT si elle l'est déjà : un compte Lecture seule qui change son
   * mot de passe ne demande jamais `GET /accounts` (l'agent le consignerait comme un accès refusé).
   */
  function reloadIfRead() {
    if (accounts.of(serverId())) void accounts.load(serverId());
  }

  async function perform(
    call: () => Promise<AccountOutcome>,
    onDone: (outcome: Extract<AccountOutcome, { kind: "done" }>) => void,
    rereads = true,
  ): Promise<AccountReport> {
    const result = await action.run(call, { unknownMessage: "accounts.unknownResult" });
    if (!result) return { kind: "failed" };
    if (result.kind === "done") {
      onDone(result);
      if (rereads) reloadIfRead();
      return { kind: "done", sessionsClosed: result.sessionsClosed };
    }
    if (result.kind === "refused") {
      // La liste affichée est périmée (compte disparu, rôle changé ailleurs) : on la relit.
      if (["not_found", "last_admin", "forbidden"].includes(result.refusal.kind)) reloadIfRead();
      return { kind: "refused", refusal: result.refusal };
    }
    return { kind: "unknown" };
  }

  const bridge = () => getLinkBridge();

  return {
    busy: action.busy,

    create(username: string, password: string, role: Role) {
      return perform(
        () => bridge().createAccount(serverId(), username, password, role),
        (done) => success(t("accounts.created", { username: done.account?.username ?? username })),
      );
    },

    changeRole(account: Account, role: Role) {
      return perform(
        () => bridge().changeAccountRole(serverId(), account.id, role),
        () =>
          success(t("accounts.roleChanged", { username: account.username, role: roleLabel(role) })),
      );
    },

    setPassword(account: Account, password: string) {
      return perform(
        () => bridge().setAccountPassword(serverId(), account.id, password),
        () => success(t("accounts.passwordChanged")),
      );
    },

    changeOwnPassword(current: string, password: string) {
      return perform(
        () => bridge().changeOwnPassword(serverId(), current, password),
        () => success(t("accounts.passwordChanged")),
        // Changer son mot de passe ne change pas la liste.
        false,
      );
    },

    closeSessions(account: Account) {
      return perform(
        () => bridge().closeAccountSessions(serverId(), account.id),
        () => success(t("accounts.sessionsClosed", { username: account.username })),
      );
    },

    /** `confirmation` : l'identifiant retapé quand on supprime son propre compte (BR-ACCT-012). */
    remove(account: Account, confirmation: string | null = null) {
      return perform(
        () => bridge().deleteAccount(serverId(), account.id, confirmation),
        () =>
          success(
            t(confirmation === null ? "accounts.removed" : "accounts.removedOwn", {
              username: account.username,
            }),
          ),
      );
    },

    /** Un refus montré en notification (action de ligne, hors boîte de dialogue). */
    toastRefusal(report: AccountReport, self = false) {
      if (report.kind !== "refused") return;
      toasts.push({
        kind: report.refusal.kind === "session_ended" ? "info" : "error",
        message: refusalMessage(report.refusal, self),
      });
    },
  };
}
