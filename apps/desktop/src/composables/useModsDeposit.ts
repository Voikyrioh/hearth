import { computed, ref } from "vue";
import { type ActReport, useReauth } from "@/composables/useReauth";
import { useServerAction } from "@/composables/useServerAction";
import { logUiError } from "@/errors/report";
import { t } from "@/i18n";
import {
  type DepositRefusal,
  failureOf,
  getLinkBridge,
  type ModCheckFile,
  type ModsReadRefusal,
  type Transfer,
} from "@/link";
import { isJar, MAX_DEPOSITS, nameOfPath } from "@/services/modsDeposit";
import { useTransfersStore } from "@/stores/transfers";
import { depositRefusalText } from "@/transfers/messages";

/** Un fichier choisi pour le lot, de son choix à son envoi. */
export interface DepositItem {
  path: string;
  name: string;
  /** L'empreinte SHA-256 du fichier local ; `null` : le fichier n'a pas pu être lu. */
  sha256: string | null;
  /** L'identifiant du dépôt sur le serveur, une fois l'envoi ouvert. */
  deposit: string | null;
  /** Pourquoi ce fichier n'est pas parti (phrase), quand c'est le cas. */
  refused: string | null;
  /** Le fichier est arrivé et a passé le contrôle : sa ligne de la carte « Transferts » est retirée, il reste « Envoyé ». */
  sent?: boolean;
}

/** Ce que le contrôle a donné. */
export type CheckState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "done"; files: ModCheckFile[] }
  | {
      kind: "refused";
      refusal: ModsReadRefusal /** Les fichiers qui ne sont plus dans la zone d'attente (noms sur le poste). */;
      gone?: string[];
    }
  | { kind: "failed" };

/**
 * Déposer des mods et les faire contrôler (HRT-262, `BR-MOD-015`, `022`, `023`) : choisir jusqu'à huit fichiers `.jar`
 * (la boîte « Ouvrir » du système, la coquille lit les fichiers), calculer leur empreinte, les envoyer dans la zone
 * d'attente (un transfert reprenable chacun, mêmes règles que le pack d'une installation), puis demander le contrôle
 * quand tous les envois sont finis. Rien n'est écrit sur le service : le contrôle est une consultation. Un fichier qui
 * n'est pas un `.jar`, un neuvième fichier, un fichier illisible : dits avant tout envoi.
 */
export function useModsDeposit(serverId: () => string, key: () => string) {
  const transfers = useTransfersStore();
  const sendAction = useServerAction(serverId);
  // L'envoi est un acte `file_write`, couvert par l'élévation : sans mot de passe à saisir, il part tout de suite, sans fenêtre.
  const reauth = useReauth(serverId, () => "file_write");
  void reauth.load();

  const items = ref<DepositItem[]>([]);
  const check = ref<CheckState>({ kind: "idle" });
  const picking = ref(false);
  /** Les fichiers choisis attendent le mot de passe pour partir (la fenêtre de confirmation est ouverte). */
  const sendOpen = ref(false);
  const sendRefusal = ref<DepositRefusal | null>(null);

  const accepted = computed(() => items.value.filter((item) => item.refused === null));
  const unsent = computed(() => accepted.value.filter((item) => item.deposit === null));

  function transferOf(item: DepositItem): Transfer | undefined {
    return item.deposit === null
      ? undefined
      : transfers.of(serverId()).find((transfer) => transfer.id === item.deposit);
  }

  /** Tous les envois sont finis : le contrôle peut commencer. */
  const allSent = computed(
    () =>
      accepted.value.length > 0 &&
      accepted.value.every((item) => item.sent === true || transferOf(item)?.status === "done"),
  );
  /** Un envoi a échoué, a été annulé ou attend un geste : le contrôle attend. */
  const needsGesture = computed(() =>
    accepted.value.some((item) => {
      const status = transferOf(item)?.status;
      return status === "failed" || status === "cancelled" || status === "interrupted";
    }),
  );

  /** La boîte « Ouvrir » : ajoute les fichiers choisis, dit ceux qui sont refusés avant l'envoi. */
  async function choose(): Promise<void> {
    if (picking.value) return;
    picking.value = true;
    try {
      await addPaths(await getLinkBridge().pickDepositFiles());
    } catch (error) {
      if (!failureOf(error)) logUiError(error, "modsDeposit:pick");
    } finally {
      picking.value = false;
    }
  }

  /** Des fichiers glissés sur la fenêtre : mêmes règles que la boîte « Ouvrir ». */
  async function dropped(paths: string[]): Promise<void> {
    if (picking.value) return;
    picking.value = true;
    try {
      await addPaths(paths);
    } catch (error) {
      if (!failureOf(error)) logUiError(error, "modsDeposit:drop");
    } finally {
      picking.value = false;
    }
  }

  async function addPaths(paths: string[]): Promise<void> {
    {
      const added: DepositItem[] = [];
      for (const path of paths) {
        const name = nameOfPath(path);
        if (items.value.some((item) => item.path === path)) continue;
        const count = accepted.value.length + added.filter((item) => item.refused === null).length;
        let refused: string | null = null;
        let sha256: string | null = null;
        if (!isJar(path)) refused = t("mods.deposit.notJar", { name });
        else if (count >= MAX_DEPOSITS) refused = t("mods.deposit.tooMany", { name });
        else {
          sha256 = await getLinkBridge().fileSha256(path);
          if (sha256 === null) refused = t("mods.deposit.unreadable", { name });
        }
        added.push({ path, name, sha256, deposit: null, refused });
      }
      items.value = [...items.value, ...added];
      check.value = { kind: "idle" };
      if (added.some((item) => item.refused === null)) {
        // L'élévation a pu s'ouvrir (ou se fermer) depuis la dernière lecture : on relit avant de décider.
        await reauth.load();
        const direct =
          reauth.supported.value && !reauth.keyMissing.value && !reauth.needsPassword.value;
        // Un refus ou un lien tombé : la fenêtre s'ouvre, elle dit pourquoi et redemande ce qu'il faut.
        if (!direct || (await send(null)).kind !== "done") sendOpen.value = true;
      }
    }
  }

  /** Ouvre un dépôt par fichier non encore envoyé (acte `file_write`, couvert par l'élévation) et lance les envois. */
  async function send(adminPassword: string | null): Promise<ActReport> {
    sendRefusal.value = null;
    for (const item of unsent.value) {
      const outcome = await sendAction.run(() =>
        transfers.start(serverId(), item.path, adminPassword),
      );
      if (outcome === null) return sendAction.failedReport();
      if (outcome.kind === "started") {
        item.deposit = outcome.transfer.id;
        items.value = [...items.value];
        continue;
      }
      if (outcome.kind === "unknown") return { kind: "unknown" };
      sendRefusal.value = outcome.refusal;
      const refusal = outcome.refusal;
      return {
        kind: "refused",
        refusal:
          refusal.kind === "too_many_attempts"
            ? { kind: refusal.kind, retry_after_s: refusal.waitS }
            : { kind: refusal.kind },
      };
    }
    return { kind: "done" };
  }

  function sendRefusalText(): string | undefined {
    return sendRefusal.value === null ? undefined : depositRefusalText(sendRefusal.value);
  }

  /** « Contrôler les fichiers » : l'agent juge chaque dépôt contre les mods du service (consultation). */
  async function runCheck(): Promise<void> {
    const ids = accepted.value.flatMap((item) => (item.deposit === null ? [] : [item.deposit]));
    if (ids.length === 0 || check.value.kind === "checking") return;
    check.value = { kind: "checking" };
    try {
      const outcome = await getLinkBridge().checkMods(serverId(), key(), ids);
      check.value =
        outcome.kind === "ready"
          ? { kind: "done", files: outcome.files }
          : { kind: "refused", refusal: outcome.refusal };
      if (outcome.kind === "ready") await settleTransfers(ids);
      else if (outcome.refusal.kind === "deposit_unknown") {
        check.value = {
          ...check.value,
          kind: "refused",
          refusal: outcome.refusal,
          gone: await namesGone(ids),
        };
      }
    } catch (error) {
      if (!failureOf(error)) logUiError(error, "modsDeposit:check");
      check.value = { kind: "failed" };
    }
  }

  /** Quels fichiers ont quitté la zone d'attente : l'agent ne le dit pas, on le lui demande fichier par fichier. */
  async function namesGone(ids: string[]): Promise<string[]> {
    if (ids.length === 1) return [nameOfDeposit(ids[0] ?? "")];
    const gone: string[] = [];
    for (const id of ids) {
      try {
        const one = await getLinkBridge().checkMods(serverId(), key(), [id]);
        if (one.kind === "refused" && one.refusal.kind === "deposit_unknown")
          gone.push(nameOfDeposit(id));
      } catch {
        // Un échec n'invente rien : le fichier n'est simplement pas nommé.
      }
    }
    return gone;
  }

  /** Le contrôle est fait : l'envoi est fini et n'a plus rien à dire, la carte « Transferts » le retire (sur tous les onglets). */
  async function settleTransfers(ids: string[]): Promise<void> {
    for (const item of items.value) {
      if (item.deposit !== null && ids.includes(item.deposit)) item.sent = true;
    }
    items.value = [...items.value];
    for (const id of ids) {
      const transfer = transfers.of(serverId()).find((other) => other.id === id);
      if (transfer?.status === "done") await transfers.dismiss(id);
    }
  }

  /** Retire un fichier de la liste : un envoi qui court est annulé (la partie reçue part du serveur). */
  async function remove(item: DepositItem): Promise<void> {
    const transfer = transferOf(item);
    items.value = items.value.filter((other) => other !== item);
    check.value = { kind: "idle" };
    if (!transfer) return;
    if (transfers.isOpen(transfer.status)) await transfers.cancel(transfer.id);
    await transfers.dismiss(transfer.id);
  }

  /** Le dépôt d'un fichier contrôlé : son nom sur le poste, pour le dire sur l'écran du contrôle. */
  function nameOfDeposit(deposit: string): string {
    return items.value.find((item) => item.deposit === deposit)?.name ?? deposit;
  }

  function shaOf(deposit: string): string | null {
    return items.value.find((item) => item.deposit === deposit)?.sha256 ?? null;
  }

  /** Un fichier refusé au contrôle quitte la liste (l'agent l'a déjà retiré de la zone d'attente). */
  async function dropChecked(deposit: string): Promise<void> {
    const item = items.value.find((other) => other.deposit === deposit);
    if (check.value.kind === "done") {
      check.value = {
        kind: "done",
        files: check.value.files.filter((file) => file.deposit !== deposit),
      };
    }
    if (!item) return;
    items.value = items.value.filter((other) => other !== item);
    await transfers.dismiss(deposit);
  }

  /** Remet à zéro (après « Continuer » ou « Tout annuler »). */
  function reset() {
    items.value = [];
    check.value = { kind: "idle" };
    sendOpen.value = false;
    sendRefusal.value = null;
  }

  return {
    items,
    accepted,
    unsent,
    check,
    picking,
    sendOpen,
    allSent,
    needsGesture,
    transferOf,
    choose,
    dropped,
    send,
    sendRefusalText,
    runCheck,
    remove,
    dropChecked,
    nameOfDeposit,
    shaOf,
    reset,
  };
}
