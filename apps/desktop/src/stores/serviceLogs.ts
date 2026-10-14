import { defineStore } from "pinia";
import { computed, ref, shallowRef } from "vue";
import { logUiError } from "@/errors/report";
import { failureOf, getLinkBridge, type LogEnd, type LogMessage, type LogReading } from "@/link";
import {
  type Entry,
  type GapKind,
  LogBuffer,
  lineEntry,
  MAX_LINES,
  type NewEntry,
} from "@/logs/buffer";

/** Les mises à jour de l'écran sont regroupées : au plus une toutes les 100 ms, quel que soit le débit. */
export const NOTIFY_MS = 100;

export type LogsPhase = "idle" | "opening" | "reading" | "ended";
/** `live` : la vue suit le direct ; `auto` : pause posée en remontant ; `manual` : pause demandée. */
export type LogsMode = "live" | "auto" | "manual";

import { gapKindOf, type LogsFailure } from "@/logs/view";

export type { LogsFailure };

export interface LogsStatus {
  state: "live" | "ended" | "waiting" | "unknown";
  mixed: boolean;
  exitCode: number | null;
  endedAt: number | null;
  notKept: boolean;
}

/** Ce que l'écran garde en attente pendant une pause. Les lignes sont déjà rendues inoffensives. */
type Op =
  | { op: "lines"; entries: NewEntry[] }
  | { op: "gap"; gap: GapKind; count: number | null; lost: boolean }
  | { op: "restart"; at: number | null };

/**
 * La lecture des journaux d'UN service à la fois (HRT-67, ADR-0040). Ce magasin n'est JAMAIS persisté : les lignes
 * n'existent qu'ici, en mémoire, de l'ouverture à la fermeture (BR-SLOG-019). `close` les EFFACE (le tampon est
 * remplacé, pas seulement caché), à chaque sortie : onglet quitté, détail fermé, autre service, autre serveur, session
 * finie, droit retiré. Une réponse tardive d'une lecture qu'on a quittée est jetée sans être gardée. Rien d'une ligne
 * n'entre dans un rapport d'erreur : seul un texte fixe est signalé.
 *
 * Le tampon n'est pas réactif (à 1 000 lignes par seconde, une réactivité par ligne figerait l'interface) : l'écran
 * relit les entrées sur le signal `version`, émis au plus toutes les 100 ms.
 */
export const useServiceLogsStore = defineStore("serviceLogs", () => {
  const phase = ref<LogsPhase>("idle");
  const target = shallowRef<{ serverId: string; key: string } | null>(null);
  const status = shallowRef<LogsStatus | null>(null);
  const interrupted = ref(false);
  const end = shallowRef<LogEnd | null>(null);
  const failure = ref<LogsFailure | null>(null);
  /** Heure (ms) avant laquelle une nouvelle ouverture serait refusée par le frein de l'agent. */
  const retryAt = ref<number | null>(null);
  const mode = ref<LogsMode>("live");
  /** Lignes arrivées depuis la pause : exact, même au-delà de ce qui est gardé en attente. */
  const waiting = ref(0);
  const trimmed = ref(false);
  const gone = ref(false);
  /** Au moins une ligne a été reçue : « rien reçu » n'est pas « aucune ligne ». */
  const received = ref(false);
  const version = ref(0);

  let buffer = new LogBuffer();
  let ops: Op[] = [];
  let heldLines = 0;
  let heldDropped = 0;
  let reading: LogReading | null = null;
  let pendingAcks = 0;
  let ticket = 0;
  let lostGap: number | null = null;
  let notifyTimer: ReturnType<typeof setTimeout> | undefined;
  let lastNotify = 0;

  const entries = (): Entry[] => buffer.entries;
  const paused = computed(() => mode.value !== "live");
  const lineCount = () => buffer.entries.reduce((n, e) => (e.kind === "line" ? n + 1 : n), 0);

  function notify(): void {
    if (notifyTimer !== undefined) return;
    const wait = Math.max(0, lastNotify + NOTIFY_MS - Date.now());
    notifyTimer = setTimeout(() => {
      notifyTimer = undefined;
      lastNotify = Date.now();
      version.value += 1;
    }, wait);
  }

  /** Mise à jour tout de suite (un geste de l'utilisateur répond sans attendre le lot suivant). */
  function notifyNow(): void {
    clearTimeout(notifyTimer);
    notifyTimer = undefined;
    lastNotify = Date.now();
    version.value += 1;
  }

  function ack(): void {
    if (reading) reading.ack(1);
    else pendingAcks += 1;
  }

  /** Applique une opération au tampon affiché. */
  function apply(op: Op): void {
    if (op.op === "lines") {
      if (lostGap !== null) {
        // La page qui suit un `resume_lost` répète les dernières lignes du moteur : ce qui est déjà affiché et qui
        // revient est retiré, et le repère de trou avec lui s'il y a recouvrement (aucune ligne ne manque alors).
        const first = op.entries.find((e) => e.kind === "line" && e.t !== null);
        if (first?.kind === "line" && first.t !== null) buffer.dropOverlap(first.t, lostGap);
        lostGap = null;
      }
      if (op.entries.length > 0) received.value = true;
      const dropped = buffer.append(op.entries);
      if (dropped > 0) trimmed.value = true;
    } else if (op.op === "gap") {
      buffer.append([{ kind: "gap", gap: op.gap, count: op.count }]);
      lostGap = op.lost ? buffer.lastGapId() : null;
    } else {
      buffer.append([{ kind: "restart", at: op.at }]);
    }
  }

  /** Une opération arrive : appliquée tout de suite en direct, mise en attente pendant une pause. */
  function receive(op: Op): void {
    if (mode.value === "live") {
      apply(op);
      notify();
      return;
    }
    if (op.op === "lines") {
      waiting.value += op.entries.length;
      heldLines += op.entries.length;
      if (op.entries.length > 0) received.value = true;
    }
    ops.push(op);
    // Au plus 5 000 lignes en attente : les plus anciennes sont écartées, comptées (BR-SLOG-006).
    while (heldLines > MAX_LINES) {
      const first = ops.find((o) => o.op === "lines" && o.entries.length > 0);
      if (!first || first.op !== "lines") break;
      const over = Math.min(heldLines - MAX_LINES, first.entries.length);
      first.entries.splice(0, over);
      heldLines -= over;
      heldDropped += over;
    }
    notify();
  }

  function onMessage(mine: number, message: LogMessage): void {
    if (mine !== ticket) return;
    switch (message.kind) {
      case "status":
        status.value = {
          state: message.state,
          mixed: message.mixed,
          exitCode: message.exitCode,
          endedAt: message.endedAt,
          notKept: message.notKept,
        };
        phase.value = "reading";
        interrupted.value = false;
        notify();
        break;
      case "lines":
        receive({ op: "lines", entries: message.lines.map(lineEntry) });
        ack();
        break;
      case "gap":
        receive({
          op: "gap",
          gap: gapKindOf(message.reason),
          count: message.count,
          lost: message.reason === "resume_lost",
        });
        break;
      case "marker":
        if (message.marker === "restarted") receive({ op: "restart", at: message.at });
        break;
      case "interrupted":
        interrupted.value = true;
        notify();
        break;
      case "ended":
        end.value = message.end;
        phase.value = "ended";
        // Le droit est retiré ou la session finie : les lignes ne restent pas à l'écran (BR-SLOG-019).
        if (
          message.end.kind === "session_ended" ||
          (message.end.kind === "refused" && message.end.refusal === "missing_right")
        ) {
          wipe();
        }
        interrupted.value = false;
        if (message.end.kind === "refused" && message.end.retryAfterS !== null) {
          retryAt.value = Date.now() + message.end.retryAfterS * 1000;
        }
        // Une lecture finie ne tient plus de place chez l'agent ; on rend aussi la nôtre.
        reading?.close();
        reading = null;
        notifyNow();
        break;
    }
  }

  /** Efface les lignes (affichées et en attente), sans toucher à l'état de la lecture. */
  function wipe(): void {
    buffer = new LogBuffer();
    ops = [];
    heldLines = 0;
    heldDropped = 0;
    lostGap = null;
    waiting.value = 0;
    trimmed.value = false;
    received.value = false;
  }

  function reset(): void {
    wipe();
    pendingAcks = 0;
    phase.value = "idle";
    status.value = null;
    interrupted.value = false;
    end.value = null;
    failure.value = null;
    retryAt.value = null;
    mode.value = "live";
    gone.value = false;
  }

  /** Ferme la lecture et EFFACE tout : les lignes ne sont plus nulle part. */
  function close(): void {
    ticket += 1;
    clearTimeout(notifyTimer);
    notifyTimer = undefined;
    reading?.close();
    reading = null;
    target.value = null;
    reset();
    notifyNow();
  }

  /** Ouvre la lecture de ce service. Une seule à la fois : l'ouverture précédente est fermée d'abord. */
  async function open(serverId: string, key: string): Promise<void> {
    close();
    const mine = ++ticket;
    target.value = { serverId, key };
    phase.value = "opening";
    try {
      const opened = await getLinkBridge().openServiceLogs(serverId, key, (message) =>
        onMessage(mine, message),
      );
      // Quitté pendant l'ouverture : la lecture qui vient de s'ouvrir est fermée aussitôt.
      if (mine !== ticket) {
        opened.close();
        return;
      }
      // Le TS ne voit pas que `onMessage` a pu changer la phase pendant l'attente.
      if ((phase.value as LogsPhase) === "ended") {
        opened.close();
        return;
      }
      reading = opened;
      if (pendingAcks > 0) {
        opened.ack(pendingAcks);
        pendingAcks = 0;
      }
    } catch (error) {
      if (mine !== ticket) return;
      phase.value = "ended";
      const kind = failureOf(error)?.kind;
      failure.value = kind === "not_connected" ? "not_connected" : "unreachable";
      // Jamais le message d'origine : il pourrait citer ce qui revient du serveur.
      if (!kind) logUiError(new Error("ouverture des journaux en échec"), "serviceLogs:open");
      notifyNow();
    }
  }

  /** Rouvre la même lecture (« Réessayer »). */
  async function retry(): Promise<void> {
    const found = target.value;
    if (!found) return;
    await open(found.serverId, found.key);
  }

  /** Fige l'affichage : plus aucune ligne ne s'ajoute ni ne disparaît de l'écran (BR-SLOG-004). */
  function pause(kind: "auto" | "manual"): void {
    if (mode.value !== "live") {
      // Une pause demandée par un geste l'emporte sur une pause posée en remontant.
      if (kind === "manual") mode.value = "manual";
      return;
    }
    mode.value = kind;
    waiting.value = 0;
    notifyNow();
  }

  /** Reprend le direct : les lignes manquées sont insérées dans l'ordre, avec le repère du trou (BR-SLOG-004). */
  function resume(): void {
    if (mode.value === "live") return;
    mode.value = "live";
    const dropped = heldDropped;
    const held = ops;
    ops = [];
    heldLines = 0;
    heldDropped = 0;
    waiting.value = 0;
    if (dropped > 0) apply({ op: "gap", gap: "pause", count: dropped, lost: false });
    for (const op of held) apply(op);
    notifyNow();
  }

  /** Le service ne figure plus dans la liste : les lignes restent, la lecture s'arrête (BR-SLOG-015). */
  function noteGone(): void {
    if (gone.value) return;
    gone.value = true;
    reading?.close();
    reading = null;
    notifyNow();
  }

  return {
    phase,
    target,
    status,
    interrupted,
    end,
    failure,
    retryAt,
    mode,
    paused,
    waiting,
    trimmed,
    gone,
    received,
    version,
    entries,
    lineCount,
    open,
    retry,
    close,
    pause,
    resume,
    noteGone,
  };
});
