import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { logUiError } from "@/errors/report";
import { type DepositOutcome, failureOf, getLinkBridge, type Transfer } from "@/link";
import { isOpen } from "@/transfers/messages";

/** La cadence à laquelle le panneau relit les envois tant que l'un d'eux envoie (octets durables sur le serveur). */
export const POLL_MS = 400;

/**
 * Les envois de fichiers de cette session (HRT-96, `BR-FILES-019`) : la liste est TENUE par la coquille (un envoi
 * continue quand on quitte l'onglet) ; ce magasin la relit. Aucun contenu de fichier, aucun mot de passe : des
 * identifiants, des noms de fichiers choisis par la page et des nombres. Rien ne repart seul : reprendre est un
 * geste (`resume`).
 */
export const useTransfersStore = defineStore("transfers", () => {
  const items = ref<Transfer[]>([]);
  /** Une relecture est en cours : jamais deux à la fois (un lien lent ne les empile pas). */
  let reading = false;
  let timer: ReturnType<typeof setInterval> | undefined;
  let watchers = 0;

  const sending = computed(() => items.value.some((item) => item.status === "sending"));

  /** Les envois d'un serveur, du plus ancien au plus récent. */
  function of(serverId: string): Transfer[] {
    return items.value.filter((item) => item.serverId === serverId);
  }

  async function refresh(): Promise<void> {
    if (reading) return;
    reading = true;
    try {
      items.value = await getLinkBridge().readTransfers();
    } catch (error) {
      logUiError(error, "transfers.read");
    } finally {
      reading = false;
    }
  }

  /** Un envoi lancé par un autre écran apparaît au plus après ce nombre de passages quand rien n'envoie. */
  const IDLE_EVERY = 5;
  let idleTicks = 0;

  function arm() {
    if (timer !== undefined || watchers === 0) return;
    timer = setInterval(() => {
      // Tant qu'un envoi envoie, les octets durables avancent : relire à chaque passage. Sinon, une relecture
      // sur cinq (la liste est tenue par la coquille, un autre écran a pu lancer un envoi).
      idleTicks += 1;
      if (sending.value || idleTicks % IDLE_EVERY === 0) void refresh();
    }, POLL_MS);
  }

  function disarm() {
    if (timer !== undefined) clearInterval(timer);
    timer = undefined;
  }

  /** Un écran montre les transferts : relire tout de suite, puis tant que quelque chose envoie. */
  async function watch(): Promise<() => void> {
    watchers += 1;
    await refresh();
    arm();
    return () => {
      watchers = Math.max(0, watchers - 1);
      if (watchers === 0) disarm();
    };
  }

  /** Ouvre un dépôt et lance l'envoi ; l'issue (ouvert, refusé, inconnu) est rendue telle quelle. */
  async function start(
    serverId: string,
    path: string,
    adminPassword: string | null,
  ): Promise<DepositOutcome> {
    const outcome = await getLinkBridge().startDeposit(serverId, path, adminPassword);
    if (outcome.kind === "started") {
      await refresh();
      arm();
    }
    return outcome;
  }

  /** La boîte « Ouvrir » du système : le chemin choisi, ou `null` si le compte l'annule (sans message). */
  async function pick(): Promise<string | null> {
    return getLinkBridge().pickDepositFile();
  }

  async function act(gesture: (id: string) => Promise<void>, id: string): Promise<void> {
    try {
      await gesture(id);
    } catch (error) {
      logUiError(failureOf(error) ?? error, "transfers.act");
    }
    await refresh();
    if (sending.value) arm();
  }

  const resume = (id: string) => act((id2) => getLinkBridge().resumeTransfer(id2), id);
  const cancel = (id: string) => act((id2) => getLinkBridge().cancelTransfer(id2), id);
  const dismiss = (id: string) => act((id2) => getLinkBridge().dismissTransfer(id2), id);

  function reset() {
    disarm();
    watchers = 0;
    items.value = [];
  }

  return {
    items,
    sending,
    of,
    refresh,
    watch,
    start,
    pick,
    resume,
    cancel,
    dismiss,
    reset,
    isOpen,
  };
});
