import { classifyLine, normalizeLine, signedClass } from "@/services/consoleLine";
import type {
  ConsoleOutcome,
  ConsoleRefusal,
  ConsoleReply,
  ConsoleSendOutcome,
  ConsoleView,
  PlayersOutcome,
  PlayersView,
} from "./console";
import type { SimulatedServices } from "./simulated-services";

/**
 * La console d'un serveur de jeu du pont SIMULÉ (navigateur de développement, Vitest, Playwright) : un petit agent qui
 * juge une commande COMME LE VRAI (`crates/hearth-agent/src/application/console.rs`) : forme, verbe refusé, classe
 * signée contre la classe rejugée, état du jeu, débit (20 par minute et par serveur), puis la réponse que l'appelant a
 * posée (`reply`). Aucun jeu n'est joint : rien ne part nulle part. Jamais livré (derrière `import.meta.env.DEV`).
 */

/** La politique de Minecraft telle que la fiche la déclare (BR-CONSOLE-004). */
export const SIM_MINECRAFT_POLICY: ConsoleView["policy"] = {
  refused: [
    { verb: "stop", reason: "use_hearth_stop" },
    { verb: "restart", reason: "use_hearth_restart" },
    { verb: "reload", reason: "breaks_server" },
  ],
  sensitive: [
    {
      verb: "op",
      title: "Donner les droits d'opérateur à {joueur} ?",
      sentence:
        "Un opérateur peut taper toutes les commandes du jeu depuis sa partie : changer le mode de jeu, bannir, donner des objets, modifier le monde. Il garde ces droits jusqu'à ce que vous les retiriez.",
      target: "first_arg",
    },
    {
      verb: "ban",
      title: "Bannir {joueur} ?",
      sentence: "Ce joueur ne pourra plus entrer sur le serveur.",
      target: "first_arg",
    },
    {
      verb: "whitelist",
      title: "Changer la liste blanche ?",
      sentence: "La liste blanche décide qui peut entrer sur le serveur.",
      target: "none",
    },
  ],
};

export function simConsoleView(patch: Partial<ConsoleView> = {}): ConsoleView {
  return {
    available: true,
    unavailableReason: null,
    state: "ready",
    canWrite: true,
    policy: SIM_MINECRAFT_POLICY,
    quick: [
      { id: "save", label: "Enregistrer le monde", text: "save-all", mode: "send" },
      { id: "say", label: "Écrire à tous les joueurs", text: "say ", mode: "fill" },
      { id: "day", label: "Passer au jour", text: "time set day", mode: "send" },
    ],
    limits: { maxChars: 256, perMinute: 20, replyTimeoutS: 10 },
    ...patch,
  };
}

export class SimulatedConsole {
  private readonly views = new Map<string, ConsoleView>();
  private readonly playersByService = new Map<string, PlayersView>();
  private readonly sentAt = new Map<string, number[]>();
  /** Les réponses du jeu par verbe ; sans réponse posée, le jeu répond une ligne vide. */
  private readonly replies = new Map<string, ConsoleReply>();
  /** Les commandes reçues (texte tapé et classe signée), dans l'ordre. */
  readonly received: { serverId: string; key: string; command: string; commandClass: number }[] =
    [];
  /** Les lectures des joueurs, dans l'ordre. */
  readonly playersRead: { serverId: string; key: string }[] = [];
  /** `true` : la lecture des joueurs échoue (lien sans réponse). */
  failPlayers = false;
  /** `true` : la console répond « injoignable » à la prochaine commande. */
  unreachable = false;

  constructor(
    private readonly services: SimulatedServices,
    private readonly now: () => number = () => Date.now(),
  ) {}

  private slot(serverId: string, key: string): string {
    return `${serverId}/${key}`;
  }

  seed(serverId: string, key: string, view: ConsoleView): void {
    this.views.set(this.slot(serverId, key), view);
  }

  setState(serverId: string, key: string, state: ConsoleView["state"]): void {
    const view = this.views.get(this.slot(serverId, key));
    if (view) this.views.set(this.slot(serverId, key), { ...view, state });
  }

  seedPlayers(serverId: string, key: string, view: PlayersView): void {
    this.playersByService.set(this.slot(serverId, key), view);
  }

  /** La réponse que le jeu donnera à ce verbe. */
  reply(verb: string, reply: ConsoleReply): void {
    this.replies.set(verb, reply);
  }

  describe(serverId: string, key: string): ConsoleOutcome {
    const found = this.services.find(serverId, key);
    if (!found || found.origin !== "hearth") {
      return { kind: "refused", refusal: { kind: "service_not_found" } };
    }
    const view = this.views.get(this.slot(serverId, key));
    if (!view) {
      return {
        kind: "ready",
        view: simConsoleView({
          available: false,
          unavailableReason: found.adopted !== null ? "adopted" : "no_console",
        }),
      };
    }
    return { kind: "ready", view: structuredClone(view) };
  }

  players(serverId: string, key: string): PlayersOutcome {
    this.playersRead.push({ serverId, key });
    const found = this.services.find(serverId, key);
    if (!found || found.origin !== "hearth") {
      return { kind: "refused", refusal: { kind: "service_not_found" } };
    }
    const view = this.playersByService.get(this.slot(serverId, key)) ?? {
      state: this.views.has(this.slot(serverId, key)) ? "unavailable" : "no_console",
      online: null,
      max: null,
      names: [],
      hiddenNames: 0,
      moreNames: 0,
      readAt: null,
    };
    return { kind: "ready", view: structuredClone(view as PlayersView) };
  }

  send(serverId: string, key: string, command: string, commandClass: number): ConsoleSendOutcome {
    this.received.push({ serverId, key, command, commandClass });
    const refuse = (refusal: ConsoleRefusal): ConsoleSendOutcome => ({ kind: "refused", refusal });
    const found = this.services.find(serverId, key);
    if (!found || found.origin !== "hearth") return refuse({ kind: "service_not_found" });
    if (found.adopted !== null) return refuse({ kind: "service_adopted" });
    const view = this.views.get(this.slot(serverId, key));
    if (!view || !view.available) return refuse({ kind: "not_available" });
    const normalized = normalizeLine(command, view.limits?.maxChars);
    if (!normalized.ok) return refuse({ kind: "invalid", reason: normalized.error });
    const judged = classifyLine(view.policy, normalized.line);
    if (judged.kind === "refused") return refuse({ kind: "verb_refused", reason: judged.reason });
    if (signedClass(judged) !== commandClass) return refuse({ kind: "class_mismatch" });
    if (view.state === "stopped") return refuse({ kind: "not_running" });
    if (view.state === "starting") return refuse({ kind: "starting" });
    if (view.state === "busy") return refuse({ kind: "operation_in_progress" });
    const stamp = this.now();
    const recent = (this.sentAt.get(this.slot(serverId, key)) ?? []).filter(
      (at) => stamp - at < 60_000,
    );
    const perMinute = view.limits?.perMinute ?? 20;
    if (recent.length >= perMinute) {
      const wait = Math.max(1, Math.ceil((60_000 - (stamp - (recent[0] ?? stamp))) / 1000));
      return refuse({ kind: "rate_limited", retry_after_s: wait });
    }
    recent.push(stamp);
    this.sentAt.set(this.slot(serverId, key), recent);
    if (this.unreachable) {
      this.unreachable = false;
      return refuse({ kind: "unreachable" });
    }
    const reply = this.replies.get(
      judged.kind === "sensitive" ? judged.verb : verbOfLine(normalized.line),
    ) ?? {
      outcome: "empty",
      reply: null,
      truncated: false,
    };
    return { kind: "replied", reply };
  }
}

function verbOfLine(line: string): string {
  return (line.split(" ")[0] ?? "").toLowerCase();
}
