import { GIB } from "@/resources";
import type { FicheDetail, FicheQuestion, FicheSetting, FicheTheme } from "./catalog";
import { TERRARIA_SETTINGS, TERRARIA_THEMES } from "./simulated-terraria-settings";

/**
 * Les fiches du pont SIMULÉ (navigateur de développement, Vitest, Playwright). Jamais livré (derrière
 * `import.meta.env.DEV`).
 *
 * - `terraria()` a la forme RÉELLE de `GET /catalog/terraria` d'un agent d'essai (fiche version 2 de
 *   `crates/hearth-agent/catalog/terraria/`) : six réglages essentiels, toutes les ressources « non mesurées », aucune
 *   variante, aucun contrat, un port ;
 * - `rich()` est une fiche d'essai qui porte tout ce qu'une fiche peut offrir et que Terraria n'a pas : des variantes,
 *   un contrat, un port facultatif, des ressources mesurées, au repos et annoncées par l'éditeur, un jeton de serveur.
 *   Elle sert à prouver que l'écran n'a aucune règle propre à un jeu.
 */

const DIGEST_TERRARIA = "7c1f0d5a9e3b24c6a8d0f1e2b3a495867c1f0d5a9e3b24c6a8d0f1e2b3a49586";
const DIGEST_RICH = "e4b2a1c3d5f60718293a4b5c6d7e8f90e4b2a1c3d5f60718293a4b5c6d7e8f90";

function question(
  id: string,
  label: string,
  help: string,
  rest: Partial<FicheQuestion> & Pick<FicheQuestion, "kind">,
): FicheQuestion {
  return {
    id,
    label,
    help,
    secret: rest.kind === "secret",
    proposed: null,
    choices: [],
    min: null,
    max: null,
    minLen: null,
    maxLen: null,
    charset: null,
    frozenAfterCreation: false,
    variants: [],
    secretKind: null,
    obtainUrl: null,
    ...rest,
  };
}

export function terraria(): FicheDetail {
  return {
    card: {
      id: "terraria",
      name: "Terraria",
      summary: "Le serveur officiel de Terraria, sans mod.",
      version: 2,
      status: "preview",
      installable: true,
    },
    digest: DIGEST_TERRARIA,
    image: { repository: "ryshe/terraria", tag: "vanilla-1.4.5.8", digest: null },
    ownerUid: 0,
    ownerGid: 0,
    variants: [],
    ports: [
      {
        id: "game",
        container: 7777,
        protocol: "tcp",
        role: "game",
        publish: "always",
        defaultHost: 7777,
        mapping: "nat",
        onlyVariants: [],
      },
    ],
    folders: [
      {
        id: "world",
        role: "world",
        target: "/root/.local/share/Terraria/Worlds",
        backup: true,
        onlyVariants: [],
      },
      { id: "config", role: "config", target: "/config", backup: true, onlyVariants: [] },
    ],
    agreements: [],
    // Les questions de l'installation sont les réglages essentiels que la fiche n'impose pas (BR-FICHE-038).
    questions: structuredClone(TERRARIA_SETTINGS)
      .filter((setting) => setting.level === "essential" && setting.imposedWhy === null)
      .map((setting) => setting.question),
    resources: {
      cpu: { minMillis: null, recommendedMillis: null, evidence: { kind: "unmeasured" } },
      memory: { minBytes: null, recommendedBytes: null, evidence: { kind: "unmeasured" } },
      disk: { installBytes: null, evidence: { kind: "unmeasured" } },
    },
    firstStart: { readyTimeoutSeconds: 1800, worldGeneration: true, downloads: [], stages: [] },
    stop: { graceSeconds: 10, worldSaved: "by_sequence", sequenceTested: false },
    imposed: [
      {
        label: "Nom du fichier du monde",
        value: null,
        why: "Hearth le déduit du nom du monde pour que le jeu retrouve son monde à chaque démarrage.",
      },
      {
        label: "Port du serveur",
        value: "7777",
        why: "Le port du jeu dans le service : celui que vous choisissez est relié à celui-ci.",
      },
      {
        label: "Ouverture automatique du port (UPnP)",
        value: "désactivée",
        why: "Un service ne modifie pas la box de votre réseau.",
      },
    ],
    themes: structuredClone(TERRARIA_THEMES),
    settings: structuredClone(TERRARIA_SETTINGS),
    settingsUnreadable: 0,
    unreadable: 0,
  };
}

export function rich(): FicheDetail {
  return {
    card: {
      id: "essai",
      name: "Jeu d'essai",
      summary: "Une fiche d'essai qui offre tout ce qu'une fiche peut offrir.",
      version: 3,
      status: "preview",
      installable: true,
    },
    digest: DIGEST_RICH,
    image: {
      repository: "example/essai",
      tag: "2.4.1",
      digest: "sha256:0a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f9",
    },
    ownerUid: 1000,
    ownerGid: 1000,
    variants: [
      {
        id: "vanilla",
        label: "Sans mod",
        help: "Le serveur du jeu, tel que l'éditeur le publie.",
        default: true,
      },
      {
        id: "modded",
        label: "Avec mods",
        help: "Le serveur avec un chargeur de mods.",
        default: false,
      },
    ],
    ports: [
      {
        id: "game",
        container: 25565,
        protocol: "tcp",
        role: "game",
        publish: "always",
        defaultHost: 25565,
        mapping: "nat",
        onlyVariants: [],
      },
      {
        id: "query",
        container: 25566,
        protocol: "udp",
        role: "query",
        publish: "optional",
        defaultHost: 25566,
        mapping: "nat",
        onlyVariants: [],
      },
      // Un port que seule la variante avec mods publie, et que le jeu annonce lui-même : même nombre partout.
      {
        id: "mods_port",
        container: 25575,
        protocol: "tcp",
        role: "voice",
        publish: "always",
        defaultHost: 25575,
        mapping: "same_number",
        onlyVariants: ["modded"],
      },
    ],
    folders: [
      { id: "world", role: "world", target: "/data/world", backup: true, onlyVariants: [] },
      { id: "logs", role: "logs", target: "/data/logs", backup: false, onlyVariants: [] },
      // Un dossier que seule la variante avec mods demande.
      { id: "mods", role: "mods", target: "/data/mods", backup: true, onlyVariants: ["modded"] },
    ],
    agreements: [
      {
        id: "licence",
        title: "Contrat de licence de l'éditeur",
        url: "https://example.org/licence",
        sentence: "L'éditeur exige que vous acceptiez son contrat avant d'installer son serveur.",
      },
    ],
    questions: richQuestions(),
    resources: {
      cpu: {
        minMillis: 1000,
        recommendedMillis: 2000,
        evidence: { kind: "measured" },
      },
      memory: {
        minBytes: 2 * GIB,
        recommendedBytes: 4 * GIB,
        evidence: { kind: "measured_idle" },
      },
      disk: {
        installBytes: 8 * GIB,
        evidence: { kind: "publisher", url: "https://example.org/configuration" },
      },
    },
    firstStart: {
      readyTimeoutSeconds: 900,
      worldGeneration: false,
      downloads: [
        {
          what: "fichiers de jeu",
          source: "les serveurs de l'éditeur",
          bytes: 6 * GIB,
          evidence: { kind: "publisher", url: "https://example.org/taille" },
        },
      ],
      stages: ["Téléchargement des fichiers de jeu", "Chargement du monde"],
    },
    stop: { graceSeconds: 30, worldSaved: "by_image_on_signal", sequenceTested: true },
    imposed: [
      {
        label: "Mode hors ligne",
        value: "désactivé",
        why: "Le serveur vérifie le compte de chaque joueur.",
      },
    ],
    ...richSettings(),
    unreadable: 0,
  };
}

function setting(
  theme: string,
  level: FicheSetting["level"],
  key: string,
  made: FicheQuestion,
  rest: Partial<Omit<FicheSetting, "question" | "key" | "theme" | "level">> = {},
): FicheSetting {
  return {
    question: made,
    key,
    theme,
    level,
    gameDefault: null,
    imposedWhy: null,
    since: null,
    until: null,
    replacedBy: null,
    list: null,
    ...rest,
  };
}

/**
 * Le catalogue de réglages de la fiche d'essai : tout ce qu'un réglage peut être et que Terraria n'a pas (une liste
 * d'identifiants, un texte opaque, un choix long, un interrupteur, un réglage imposé, un réglage d'une variante, un défaut
 * non vérifié, un réglage ajouté dans une version).
 */
function richSettings(): Pick<FicheDetail, "themes" | "settings" | "settingsUnreadable"> {
  const themes: FicheTheme[] = [
    { id: "server", label: "Serveur", order: 1 },
    { id: "gameplay", label: "Partie", order: 2 },
    { id: "mods", label: "Mods", order: 3 },
  ];
  const base = richQuestions();
  const byId = (id: string) => base.find((item) => item.id === id) as FicheQuestion;
  const settings: FicheSetting[] = [
    setting("server", "essential", "server-name", byId("server_name"), { gameDefault: "Serveur" }),
    setting("server", "essential", "slots", byId("slots"), { gameDefault: "10" }),
    setting("server", "essential", "token", byId("server_token")),
    setting(
      "server",
      "common",
      "motd",
      question("motd", "Message d'accueil", "Le message que chaque joueur voit en arrivant.", {
        kind: "text",
        charset: "line",
        minLen: 0,
        maxLen: 60,
      }),
      { gameDefault: "" },
    ),
    setting(
      "server",
      "advanced",
      "region",
      question("region", "Région", "La région annoncée dans la liste publique des serveurs.", {
        kind: "choice",
        proposed: "eu",
        choices: [
          { value: "eu", label: "Europe" },
          { value: "na", label: "Amérique du Nord" },
          { value: "sa", label: "Amérique du Sud" },
          { value: "as", label: "Asie" },
          { value: "oc", label: "Océanie" },
          { value: "af", label: "Afrique" },
        ],
      }),
      { gameDefault: "eu" },
    ),
    setting(
      "server",
      "advanced",
      "online-mode",
      question(
        "online_mode",
        "Vérification des comptes",
        "Le serveur vérifie le compte de chaque joueur.",
        {
          kind: "bool",
          choices: [
            { value: "true", label: "true" },
            { value: "false", label: "false" },
          ],
        },
      ),
      { gameDefault: "true", imposedWhy: "Le serveur vérifie le compte de chaque joueur." },
    ),
    setting("gameplay", "essential", "mode", byId("mode"), { gameDefault: "survival" }),
    setting("gameplay", "essential", "pvp", byId("pvp"), { gameDefault: "false" }),
    setting(
      "gameplay",
      "common",
      "whitelist",
      question("whitelist", "Liste blanche", "Seuls les joueurs de la liste peuvent entrer.", {
        kind: "bool",
        proposed: "false",
        choices: [
          { value: "true", label: "true" },
          { value: "false", label: "false" },
        ],
      }),
      { gameDefault: "false" },
    ),
    setting(
      "gameplay",
      "advanced",
      "damage-scale",
      question("damage_scale", "Force des dégâts", "Multiplie les dégâts, de 0 à 10.", {
        kind: "int",
        proposed: "5",
        min: 0,
        max: 10,
      }),
      { gameDefault: "5" },
    ),
    setting(
      "gameplay",
      "advanced",
      "sandbox",
      question(
        "sandbox_code",
        "Code des options de partie",
        "Réglez vos choix dans le jeu, copiez le code et collez-le ici. Hearth ne peut ni le lire ni le fabriquer.",
        { kind: "opaque", maxLen: 400 },
      ),
      { since: "3.0" },
    ),
    setting(
      "gameplay",
      "advanced",
      "old-difficulty",
      question(
        "old_difficulty",
        "Ancienne difficulté",
        "Remplacée par le code des options de partie.",
        {
          kind: "int",
          min: 0,
          max: 5,
        },
      ),
      { until: "3.0", replacedBy: "sandbox_code" },
    ),
    setting("mods", "essential", "mod-pack", byId("mod_pack")),
    setting(
      "mods",
      "common",
      "mod-ids",
      question("mod_ids", "Numéros des mods", "Les numéros des mods que le jeu charge.", {
        kind: "id_list",
        variants: ["modded"],
      }),
      { list: { itemCharset: "digits", separator: ",", maxItems: 5 } },
    ),
  ];
  return { themes, settings, settingsUnreadable: 0 };
}

/** Les questions d'installation de la fiche d'essai. */
function richQuestions(): FicheQuestion[] {
  return [
    question("server_name", "Nom du serveur", "Le nom que voient les joueurs.", {
      kind: "text",
      proposed: "Mon serveur",
      charset: "line",
      minLen: 1,
      maxLen: 40,
    }),
    question(
      "slots",
      "Nombre de places",
      "Combien de joueurs peuvent se connecter en même temps.",
      { kind: "int", proposed: "10", min: 2, max: 64 },
    ),
    question("mode", "Mode de jeu", "Le mode de la partie.", {
      kind: "choice",
      proposed: "survival",
      choices: [
        { value: "survival", label: "Survie" },
        { value: "creative", label: "Créatif" },
      ],
      frozenAfterCreation: true,
    }),
    question("pvp", "Combat entre joueurs", "Autorise les joueurs à se battre entre eux.", {
      kind: "bool",
      proposed: "false",
      // Les deux valeurs que le jeu comprend : activé, puis désactivé.
      choices: [
        { value: "true", label: "true" },
        { value: "false", label: "false" },
      ],
    }),
    question(
      "server_token",
      "Jeton de serveur",
      "Le jeton propre à ce serveur, créé sur le site de la plateforme.",
      {
        kind: "secret",
        secretKind: "server_token",
        minLen: 1,
        maxLen: 64,
        obtainUrl: "https://example.org/jetons",
      },
    ),
    question("mod_pack", "Pack de mods", "Le pack chargé au démarrage.", {
      kind: "text",
      charset: "arg_safe",
      proposed: "base",
      minLen: 1,
      maxLen: 30,
      variants: ["modded"],
    }),
  ];
}
