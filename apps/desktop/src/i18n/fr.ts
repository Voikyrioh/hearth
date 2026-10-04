/**
 * Textes de l'interface : français, tutoiement, pas de tiret cadratin.
 * Aucun texte en dur dans les composants : tout passe par `t()`.
 */
export const fr = {
  app: {
    name: "Hearth",
  },
  rail: {
    label: "Serveurs",
    home: "Accueil",
    addServer: "Ajouter un serveur",
    settings: "Réglages",
  },
  welcome: {
    title: "Bienvenue dans Hearth",
    text: "Ajoute ton premier serveur pour commencer.",
    addServer: "Ajouter un serveur",
    addServerSoon: "Bientôt disponible",
  },
  settings: {
    title: "Réglages",
    sectionGeneral: "Général",
    launchAtStartup: "Lancer Hearth au démarrage de Windows",
    launchAtStartupHelp: "Hearth s'ouvrira dans la zone de notification au démarrage de Windows.",
    toggleUnavailable: "Disponible dès que tes réglages sont lus.",
    logs: "Journaux",
    logsHelp: "Les journaux aident à comprendre un problème.",
    openLogs: "Ouvrir le dossier des journaux",
    version: "Version",
    versionUnavailable: "indisponible",
    loadError: "Impossible de lire tes réglages.",
    saveError: "Impossible de changer ce réglage. Réessaie.",
  },
  errors: {
    store: "Impossible de lire ou d'enregistrer tes réglages.",
    autostart: "Windows n'a pas pris en compte ce changement. Réessaie.",
    logs: "Impossible d'ouvrir le dossier des journaux.",
  },
} as const;

type Leaves<T, Prefix extends string = ""> = {
  [K in keyof T & string]: T[K] extends string ? `${Prefix}${K}` : Leaves<T[K], `${Prefix}${K}.`>;
}[keyof T & string];

/** Clé d'un texte, par chemin pointé (`welcome.title`). Une clé inconnue ne compile pas. */
export type MessageKey = Leaves<typeof fr>;
