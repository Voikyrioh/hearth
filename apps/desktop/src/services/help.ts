import { t } from "@/i18n";
import type { EngineInfo } from "@/link";

/**
 * L'aide à l'état non prêt du moteur (BR-ENGINE-005, 007, 008, 017) : du texte à recopier, adapté à la
 * distribution du serveur (`host.osId`, `host.osLike`, `host.declarative`). Hearth ne l'exécute jamais, il
 * n'inclut aucun secret ni aucune donnée d'un compte Hearth (le compte de l'agent est celui que l'état
 * « Accès refusé » nomme déjà). Aucun bouton d'installation, de réparation ni de démarrage (BR-ENGINE-006).
 */
export type Family = "debian" | "fedora" | "arch" | "nixos";

const BY_ID: Record<string, Family> = {
  debian: "debian",
  ubuntu: "debian",
  raspbian: "debian",
  linuxmint: "debian",
  pop: "debian",
  kali: "debian",
  fedora: "fedora",
  rhel: "fedora",
  centos: "fedora",
  rocky: "fedora",
  almalinux: "fedora",
  ol: "fedora",
  arch: "arch",
  manjaro: "arch",
  endeavouros: "arch",
  nixos: "nixos",
};

/** La famille de distribution reconnue, ou `null` : alors l'aide est un renvoi à la documentation, sans commande. */
export function familyOf(host: EngineInfo["host"]): Family | null {
  if (host.declarative) return "nixos";
  for (const id of [host.osId, ...host.osLike]) {
    const family = BY_ID[id.trim().toLowerCase()];
    if (family) return family;
  }
  return null;
}

export type HelpKind = "install" | "start" | "access";

/** Ce que l'interface montre en aide. `docs` : renvoi à la documentation, aucune commande. */
export type Help =
  | { kind: "command"; title: string; text: string; config: false }
  | { kind: "config"; title: string; text: string; config: true }
  | { kind: "docs" };

/**
 * L'aide d'une étape pour ce serveur. Seule l'installation dépend de la distribution : démarrer le moteur
 * (`systemctl`, sous systemd) et autoriser un compte (`usermod`) valent pour toutes. Un serveur déclaratif
 * (NixOS) reçoit toujours un texte de configuration. Distribution inconnue à l'installation, ou pas de
 * systemd pour démarrer : le renvoi à la documentation, jamais une commande inventée.
 */
export function helpFor(kind: HelpKind, engine: EngineInfo): Help {
  const family = familyOf(engine.host);
  const account = engine.agentAccount;
  if (family === "nixos") {
    return kind === "access"
      ? {
          kind: "config",
          title: t("engine.helpAccess"),
          text: t("engine.help.nixosAccess", { account }),
          config: true,
        }
      : {
          kind: "config",
          title: t("engine.helpConfig"),
          text: t("engine.help.nixosConfig"),
          config: true,
        };
  }
  if (kind === "access") {
    return {
      kind: "command",
      title: t("engine.helpAccess"),
      text: t("engine.help.access", { account }),
      config: false,
    };
  }
  if (kind === "start") {
    if (engine.host.init !== "systemd") return { kind: "docs" };
    return {
      kind: "command",
      title: t("engine.helpStart"),
      text: t("engine.help.start"),
      config: false,
    };
  }
  if (family === null || engine.host.init !== "systemd") return { kind: "docs" };
  const install: Record<Exclude<Family, "nixos">, string> = {
    debian: t("engine.help.debianInstall"),
    fedora: t("engine.help.fedoraInstall"),
    arch: t("engine.help.archInstall"),
  };
  return {
    kind: "command",
    title: t("engine.helpInstall"),
    text: install[family],
    config: false,
  };
}
