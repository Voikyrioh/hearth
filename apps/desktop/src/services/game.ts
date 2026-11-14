import { safeText } from "@/audit/text";
import type { ProgressStep } from "@/components/molecules/ProgressSteps.vue";
import { type MessageKey, t } from "@/i18n";
import type { FicheStop, GamePort, GameView, ServiceOperation } from "@/link";

/**
 * Le quotidien d'un serveur de jeu (HRT-104, `BR-JEU-004`, `010`, `012`, `017` à `021`, `027`) : des fonctions pures, sans
 * E/S. Le client ne contient AUCUNE connaissance d'un jeu : le nom du jeu, la façon dont son monde est enregistré à
 * l'arrêt et ses ports viennent de l'agent (`GameView`, lue dans la fiche de sa construction). Aucun texte ne vient de
 * l'agent : il envoie des CODES, l'interface les met en phrases. Un nom de jeu venu du serveur est un texte inoffensif
 * (`safeText`), jamais du HTML.
 */

/** Comment l'arrêt de ce jeu se passe (BR-JEU-002, BR-JEU-034) : l'ordre dans lequel Hearth sait le faire. */
export type StopWay =
  /** Hearth demande au jeu d'enregistrer son monde (suite essayée), attend, puis arrête. */
  | "sequence"
  /** L'image enregistre le monde quand elle reçoit le signal d'arrêt : aucun ordre à envoyer. */
  | "image"
  /** Hearth ne peut rien demander au jeu (suite non essayée, jeu qui n'enregistre pas) : le compte décidera. */
  | "ask";

export function stopWay(stop: FicheStop): StopWay {
  if (stop.worldSaved === "by_image_on_signal") return "image";
  if (stop.worldSaved === "by_sequence" && stop.sequenceTested) return "sequence";
  return "ask";
}

/** Le nom du jeu pour une phrase : texte du serveur rendu inoffensif. */
export function gameName(view: Pick<GameView, "name">): string {
  return safeText(view.name);
}

/** Un nombre de secondes écrit (« 120 secondes »), la même formule que le délai d'arrêt du détail. */
export function secondsText(seconds: number): string {
  return t(seconds === 1 ? "serviceDetail.graceOne" : "serviceDetail.grace", { n: seconds });
}

// ── La fenêtre de confirmation d'un arrêt ou d'un redémarrage (BR-JEU-004, BR-JEU-007) ──────────────

const EFFECT_KEYS: Record<StopWay, Record<"stop" | "restart", MessageKey>> = {
  sequence: { stop: "game.effect.sequenceStop", restart: "game.effect.sequenceRestart" },
  image: { stop: "game.effect.imageStop", restart: "game.effect.imageRestart" },
  ask: { stop: "game.effect.askStop", restart: "game.effect.askRestart" },
};

/**
 * La phrase de la fiche, à la PLACE de la phrase générale « Hearth ne peut pas encore envoyer d'ordre de sauvegarde au
 * service » (BR-JEU-004). Le délai est celui de la fiche, jamais une constante de l'interface.
 */
export function gameEffectText(gesture: "stop" | "restart", view: GameView): string {
  const key = EFFECT_KEYS[stopWay(view.stop)][gesture];
  return t(key, { game: gameName(view), delay: secondsText(view.stop.graceSeconds) });
}

// ── La carte « Arrêt du jeu » (BR-JEU-010) ───────────────────────────────────────────────────────────

export interface StopCard {
  /** Ce que le jeu fait de son monde en s'arrêtant. */
  main: string;
  /** La phrase qui reste écrite en permanence : un arrêt hors de Hearth n'enregistre pas le monde. `null` : sans objet. */
  outside: string | null;
  grace: string;
}

export function stopCardOf(view: GameView): StopCard {
  const way = stopWay(view.stop);
  const delay = secondsText(view.stop.graceSeconds);
  const grace = t("game.stopCard.grace", { delay });
  if (way === "image") {
    return { main: t("game.stopCard.saves", { delay }), outside: null, grace };
  }
  return {
    main: t(way === "sequence" ? "game.stopCard.asks" : "game.stopCard.cannotAsk"),
    outside: t("game.stopCard.outside"),
    grace,
  };
}

// ── Les cartes « Adresse » et « Ports à ouvrir » (BR-JEU-017 à BR-JEU-021) ────────────────────────────

const ROLE_KEYS: Record<string, MessageKey> = {
  game: "game.ports.role.game",
  query: "game.ports.role.query",
  voice: "game.ports.role.voice",
  web_admin: "game.ports.role.webAdmin",
  remote_console: "game.ports.role.remoteConsole",
  other: "game.ports.role.other",
};

/** Le rôle d'un port en mots ; un rôle que ce client ne connaît pas, ou absent, ne devient jamais un code affiché. */
export function roleText(role: string): string {
  return t(ROLE_KEYS[role] ?? "game.ports.role.unknown");
}

/** Le protocole en majuscules ; un protocole que ce client ne connaît pas est écarté du tableau. */
export function protocolText(protocol: string): string | null {
  return protocol === "tcp" || protocol === "udp" || protocol === "sctp"
    ? protocol.toUpperCase()
    : null;
}

/** Les ports montrés, dans l'ordre de leur numéro ; un port au protocole inconnu n'est pas inventé. */
export function portRows(ports: readonly GamePort[]): {
  key: string;
  port: number;
  protocol: string;
  role: string;
  changed: boolean;
}[] {
  return [...ports]
    .sort((a, b) => a.hostPort - b.hostPort || a.protocol.localeCompare(b.protocol))
    .flatMap((port) => {
      const protocol = protocolText(port.protocol);
      return protocol === null
        ? []
        : [
            {
              key: `${port.hostPort}/${port.protocol}`,
              port: port.hostPort,
              protocol,
              role: roleText(port.role),
              changed: port.changed,
            },
          ];
    });
}

/** Un port a changé par rapport à la fiche : le rappel permanent « mettez à jour votre box » (BR-JEU-019). */
export function portsChanged(ports: readonly GamePort[]): boolean {
  return ports.some((port) => port.changed);
}

/** « {port}/{protocole} » une par ligne : la liste à copier (BR-JEU-017). */
export function portsListText(ports: readonly GamePort[]): string {
  return portRows(ports)
    .map((row) => `${row.port}/${row.protocol.toLowerCase()}`)
    .join("\n");
}

export interface Address {
  /** « {adresse}:{port} » : celle que ce client utilise pour joindre le serveur, avec le port de base du jeu. */
  text: string;
  /** Le jeu utilise d'autres ports que celui-ci : la carte le dit (BR-JEU-020). */
  others: boolean;
}

/**
 * L'adresse que le propriétaire donne à ses joueurs (BR-JEU-020, 021) : celle sous laquelle ce client joint le serveur,
 * avec le port de base du jeu (le port de rôle « jeu », sinon le plus petit). `null` : aucun port publié, donc rien à
 * donner. Une adresse IPv6 s'écrit entre crochets.
 */
export function addressOf(host: string, ports: readonly GamePort[]): Address | null {
  const rows = [...ports]
    .filter((port) => protocolText(port.protocol) !== null)
    .sort((a, b) => a.hostPort - b.hostPort);
  const base = rows.find((port) => port.role === "game") ?? rows[0];
  if (!base) return null;
  const clean = safeText(host);
  const shown = clean.includes(":") && !clean.startsWith("[") ? `[${clean}]` : clean;
  const distinct = new Set(rows.map((port) => port.hostPort));
  return { text: `${shown}:${base.hostPort}`, others: distinct.size > 1 };
}

// ── La séquence d'un arrêt ou d'un redémarrage (BR-JEU-003, 005, 008) ──────────────────────────────────

/** Les étapes de la séquence, dans l'ordre du plan de l'agent. */
export type SequenceStage = "save_world" | "stop" | "start";

const STAGE_KEYS: Record<SequenceStage, MessageKey> = {
  save_world: "game.sequence.saveWorld",
  stop: "game.sequence.stop",
  start: "game.sequence.start",
};

const STAGE_OF: Record<string, SequenceStage | undefined> = {
  save_world: "save_world",
  stop: "stop",
  start: "start",
};

/** Les étapes du plan : « Sauvegarde du monde » seulement si la suite l'a (ou si l'agent l'a ouverte), « Démarrage » pour un redémarrage. */
export function stagesOf(
  operation: ServiceOperation | null,
  plan: { save: boolean; restart: boolean },
): SequenceStage[] {
  const opened = new Set<SequenceStage | undefined>([
    ...(operation?.steps ?? []).map((line) => STAGE_OF[line.step]),
    operation?.step ? STAGE_OF[operation.step] : undefined,
  ]);
  return [
    ...(plan.save || opened.has("save_world") ? (["save_world"] as const) : []),
    "stop",
    ...(plan.restart || opened.has("start") ? (["start"] as const) : []),
  ];
}

/** Les lignes de `ProgressSteps` : celles d'avant l'étape en cours sont faites, l'étape en cours est « en cours ». */
export function sequenceSteps(
  operation: ServiceOperation | null,
  plan: { save: boolean; restart: boolean },
  bar: { stage: SequenceStage; text: string | null } | null,
): ProgressStep[] {
  const stages = stagesOf(operation, plan);
  const current = operation?.step ? STAGE_OF[operation.step] : undefined;
  const index = Math.max(0, current ? stages.indexOf(current) : 0);
  return stages.map((stage, position): ProgressStep => {
    const state = position < index ? "done" : position === index ? "now" : "later";
    const step: ProgressStep = { key: stage, label: t(STAGE_KEYS[stage]), state };
    if (bar && stage === bar.stage && state === "now") step.bar = { ratio: null, text: bar.text };
    return step;
  });
}

/** Le monde a été enregistré par la suite avant l'arrêt (BR-JEU-005) : l'étape « Sauvegarde du monde » est faite. */
export function worldSaved(operation: ServiceOperation): boolean {
  return operation.steps.some((line) => line.step === "save_world" && line.state === "done");
}

/** Une opération d'arrêt ou de redémarrage d'un service (le seul genre dont une décision est celle d'un arrêt de jeu). */
export function isStopOperation(operation: Pick<ServiceOperation, "kind">): boolean {
  return operation.kind === "stop" || operation.kind === "restart";
}

/**
 * Pourquoi l'arrêt attend une décision (BR-JEU-006, BR-JEU-007) : avant tout envoi (aucune étape ouverte), Hearth ne
 * peut pas demander l'enregistrement ; après l'ordre sans confirmation, il n'a pas pu s'assurer que le monde est
 * enregistré.
 */
export function decisionReason(operation: ServiceOperation): "cannot-ask" | "not-confirmed" {
  return operation.steps.some((line) => line.step === "save_world") ||
    operation.step === "save_world"
    ? "not-confirmed"
    : "cannot-ask";
}
