import { computed, ref, watch } from "vue";
import { useNow } from "@/composables/useNow";
import type { ActReport } from "@/composables/useReauth";
import { useReauth } from "@/composables/useReauth";
import { useServerAction } from "@/composables/useServerAction";
import { type ConsoleView, getLinkBridge, type QuickCommand } from "@/link";
import {
  type Exchange,
  fieldBlock,
  lineIssueOf,
  refusalSentence,
  refusedSentence,
  replyExchange,
} from "@/services/console";
import {
  classifyLine,
  type LineClass,
  normalizeLine,
  sensitiveTitle,
  signedClass,
} from "@/services/consoleLine";

/** Au plus ce nombre de lignes dans l'échange de la session (le journal du serveur garde le reste). */
export const MAX_EXCHANGES = 200;

/** La commande en attente de confirmation : le texte tel que tapé, sa classe et ce que la fiche en dit. */
export interface Pending {
  command: string;
  class: 1 | 2;
  title: string;
  sentence: string | null;
}

/**
 * Le parcours de l'onglet « Console » (HRT-269, `BR-CONSOLE-002` à `010`) : taper une commande, la juger comme l'agent
 * (forme, classe : refusée, sensible ou libre), la confirmer par mot de passe quand la classe ou l'état de confiance
 * l'exige (une commande sensible TOUJOURS), l'envoyer par le lien (jamais rejouée : lien tombé = « résultat inconnu »,
 * regardez le journal), et garder l'échange de CETTE session (commandes et réponses ne sont ni gardées par l'agent ni
 * montrées sur un autre poste). Le texte d'une commande et la réponse du jeu ne sont écrits dans aucune trace.
 *
 * @param view ce que la console permet maintenant (`GET /services/{key}/console`), relu par l'écran qui l'accueille.
 * @param refresh relit ce que la console permet (un refus d'état la rend plus vraie).
 */
export function useServiceConsole(
  serverId: () => string,
  serviceKey: () => string,
  view: () => ConsoleView | null,
  refresh: () => void,
) {
  const action = useServerAction(serverId);
  const now = useNow();
  const typed = ref("");
  const exchanges = ref<Exchange[]>([]);
  const sending = ref(false);
  const pending = ref<Pending | null>(null);
  const refusal = ref<string | undefined>();
  /** La console dit d'attendre : le moment où la prochaine commande peut partir (ms). */
  const waitUntil = ref(0);
  let nextId = 1;

  // Une commande libre en état de confiance normal ne demande rien : l'état se lit de l'agent, jamais deviné.
  const reauth = useReauth(serverId, () => "console_command_free");
  void reauth.load();

  const maxChars = computed(() => view()?.limits?.maxChars ?? 256);
  const rateWaitS = computed(() => Math.max(0, Math.ceil((waitUntil.value - now.value) / 1000)));
  const block = computed(() => fieldBlock(view(), rateWaitS.value));

  const normalized = computed(() => normalizeLine(typed.value, maxChars.value));
  /** La classe de ce qui est tapé, jugée avec la politique de la fiche (jamais écrite ici). */
  const judged = computed<LineClass | null>(() => {
    const current = view();
    const line = normalized.value;
    return current && line.ok ? classifyLine(current.policy, line.line) : null;
  });
  /** Pourquoi « Envoyer » est inactif quand le champ est actif : la forme de la ligne. */
  const issue = computed(() => {
    if (typed.value === "") return null;
    const line = normalized.value;
    return line.ok ? null : lineIssueOf(typed.value, line.error, maxChars.value);
  });
  const canSend = computed(
    () => block.value === null && normalized.value.ok && !sending.value && pending.value === null,
  );

  function push(entry: Omit<Exchange, "id" | "at">): void {
    const next = [...exchanges.value, { id: nextId++, at: Date.now(), ...entry }];
    exchanges.value = next.slice(-MAX_EXCHANGES);
  }

  /** Envoie la commande (la classe est celle de l'écran ; l'agent la rejuge). Jamais rejoué. */
  async function perform(
    command: string,
    commandClass: 1 | 2,
    adminPassword: string | null,
  ): Promise<ActReport> {
    refusal.value = undefined;
    sending.value = true;
    try {
      const outcome = await action.run(
        () =>
          getLinkBridge().sendConsole(
            serverId(),
            serviceKey(),
            command,
            commandClass,
            adminPassword,
          ),
        {
          unknownMessage: "console.unknown",
          forbiddenMessage: "console.refusal.other",
          passwordMissingIsRefusal: true,
        },
      );
      if (!outcome) {
        if (action.passwordMissing.value) {
          return { kind: "refused", refusal: { kind: "password_required" } };
        }
        return action.failedReport();
      }
      if (outcome.kind === "replied") {
        push({ command, by: null, ...replyExchange(outcome.reply) });
        typed.value = "";
        return { kind: "done" };
      }
      if (outcome.kind === "unknown") {
        // Le lien est tombé avant la réponse : la commande a peut-être été exécutée, elle n'est JAMAIS renvoyée.
        push({
          command,
          by: null,
          outcome: "unknown",
          reply: null,
          sentence: t("console.reply.unknown"),
        });
        return { kind: "unknown" };
      }
      const reason = outcome.refusal;
      if (
        reason.kind === "wrong_password" ||
        reason.kind === "password_required" ||
        reason.kind === "too_many_attempts" ||
        reason.kind === "busy"
      ) {
        return { kind: "refused", refusal: reason };
      }
      if (reason.kind === "rate_limited") {
        waitUntil.value = Date.now() + (reason.retry_after_s ?? 60) * 1000;
      }
      if (reason.kind === "unreachable") {
        push({
          command,
          by: null,
          outcome: "unreachable",
          reply: null,
          sentence: refusalSentence(reason),
        });
        return { kind: "done" };
      }
      // L'état du jeu a changé depuis la dernière lecture : la console se relit.
      if (
        reason.kind === "not_running" ||
        reason.kind === "starting" ||
        reason.kind === "operation_in_progress"
      ) {
        refresh();
      }
      refusal.value = refusalSentence(reason);
      return { kind: "refused", refusal: { kind: "other" } };
    } finally {
      sending.value = false;
    }
  }

  /** « Envoyer » : juge la ligne, refuse, demande la confirmation ou envoie. */
  async function submit(text: string = typed.value): Promise<void> {
    const current = view();
    const line = normalizeLine(text, maxChars.value);
    if (current === null || !line.ok || block.value !== null || sending.value) return;
    const judgement = classifyLine(current.policy, line.line);
    if (judgement.kind === "refused") {
      // Jamais envoyée : la ligne est barrée, la phrase dit quoi faire à la place.
      push({
        command: text,
        by: null,
        outcome: "refused",
        reply: null,
        sentence: refusedSentence(judgement.reason),
      });
      return;
    }
    const commandClass = signedClass(judgement);
    if (commandClass === null) return;
    if (judgement.kind === "sensitive") {
      pending.value = {
        command: text,
        class: 2,
        title: sensitiveTitle(judgement.title, judgement.player),
        sentence: judgement.sentence,
      };
      return;
    }
    // Une commande libre : le mot de passe suit l'état de confiance que l'agent annonce (rien en état normal).
    await reauth.load();
    if (reauth.needsPassword.value) {
      pending.value = { command: text, class: 1, title: "", sentence: null };
      return;
    }
    await perform(text, 1, null);
  }

  /** Une commande rapide de la fiche : « envoyer » part comme une commande tapée, « remplir » pose le texte. */
  async function quick(command: QuickCommand): Promise<void> {
    if (command.mode === "fill") {
      typed.value = command.text;
      return;
    }
    await submit(command.text);
  }

  /** La fenêtre de confirmation envoie la commande avec le mot de passe saisi. */
  async function confirm(adminPassword: string | null): Promise<ActReport> {
    const current = pending.value;
    if (current === null) return { kind: "done" };
    const report = await perform(current.command, current.class, adminPassword);
    return report;
  }

  function closePending(): void {
    pending.value = null;
    refusal.value = undefined;
  }

  // Un autre service : l'échange de l'ancien ne se montre jamais sous le nouveau.
  watch([serverId, serviceKey], () => {
    exchanges.value = [];
    typed.value = "";
    pending.value = null;
  });

  return {
    typed,
    exchanges,
    sending,
    pending,
    refusal,
    block,
    judged,
    issue,
    canSend,
    maxChars,
    submit,
    quick,
    confirm,
    closePending,
  };
}

import { t } from "@/i18n";
