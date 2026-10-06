import { type Ref, ref, watch } from "vue";
import { type AccountInputCheck, getLinkBridge } from "@/link";

const EMPTY: AccountInputCheck = { username: null, password: [] };

/**
 * Verdict en direct d'un identifiant et d'un mot de passe : la règle est celle de l'agent
 * (`hearth-proto`), évaluée par la coquille sans réseau. Une réponse en retard ne remplace jamais
 * une plus récente. L'interface n'a AUCUNE copie de la règle.
 */
export function useAccountRules(username: Ref<string>, password: Ref<string>) {
  const check = ref<AccountInputCheck>(EMPTY);
  /** Faux tant que la première évaluation n'est pas revenue : le formulaire ne s'envoie pas. */
  const ready = ref(false);
  let ticket = 0;

  async function evaluate() {
    const mine = ++ticket;
    try {
      const verdict = await getLinkBridge().checkAccountInput(username.value, password.value);
      if (mine !== ticket) return;
      check.value = verdict;
      ready.value = true;
    } catch {
      if (mine === ticket) ready.value = false;
    }
  }

  watch([username, password], evaluate, { immediate: true });

  return { check, ready };
}
