/**
 * Textes de l'interface : français, tutoiement, pas de tiret cadratin.
 * Aucun texte en dur dans les composants : tout passe par `t()`.
 */
export const fr = {
  app: {
    name: "Hearth",
  },
  common: {
    comingSoon: "Bientôt disponible",
    close: "Fermer",
    cancel: "Annuler",
    confirm: "Confirmer",
    retry: "Réessayer",
    loading: "Chargement",
  },
  rail: {
    label: "Serveurs",
    home: "Accueil",
    addServer: "Ajouter un serveur",
    settings: "Réglages",
  },
  nav: {
    label: "Navigation du serveur",
    dashboard: "Tableau de bord",
    accounts: "Comptes",
    audit: "Journal d'activité",
  },
  link: {
    connected: "Connecté",
    reconnecting: "Reconnexion…",
    offline: "Hors ligne",
    sessionExpired: "Session expirée",
    accessRevoked: "Accès révoqué",
    pillLabel: "État du lien avec le serveur",
    retryNow: "Réessayer maintenant",
    offlineBanner:
      "Serveur hors ligne. Dernier contact à {time}. Nouvelle tentative automatique en cours.",
    offlineBannerNoContact: "Serveur hors ligne. Nouvelle tentative automatique en cours.",
    staleHelp: "Les données affichées proviennent de la dernière connexion et ne sont pas à jour.",
    seenSeconds: "Vu il y a {n} s",
    seenMinutes: "Vu il y a {n} min",
    seenHours: "Vu il y a {n} h",
    seenDays: "Vu il y a {n} j",
    neverSeen: "Jamais vu",
  },
  needs: {
    reconnecting: "Indisponible pendant la reconnexion au serveur.",
    offline: "Indisponible tant que le serveur est hors ligne.",
    sessionExpired: "Indisponible : ta session a expiré.",
    accessRevoked: "Indisponible : ton compte n'est plus accessible.",
    role: "Réservé aux administrateurs.",
    noServer: "Indisponible : aucun serveur sélectionné.",
  },
  operation: {
    done: "Fait pendant la coupure.",
    notExecuted: "Non exécuté. Tu peux relancer.",
    unknown: "Résultat inconnu. Vérifie l'état du serveur.",
  },
  server: {
    avatarLabel: "{name}, {state}",
    roleReadonly: "Lecture seule",
    roleAdmin: "Administrateur",
  },
  toast: {
    region: "Notifications",
    close: "Fermer la notification",
    repeated: "{n} fois",
    uiError: "Un problème est survenu dans l'interface. Il a été noté dans le journal.",
  },
  field: {
    showPassword: "Afficher le mot de passe",
    hidePassword: "Masquer le mot de passe",
  },
  errorBoundary: {
    title: "Cette page a rencontré un problème",
    text: "Le reste de l'application fonctionne toujours. Tu peux réessayer.",
    retry: "Réessayer",
  },
  pages: {
    dashboard: "Tableau de bord",
    accounts: "Comptes",
    audit: "Journal d'activité",
    addAccount: "Ajouter un compte",
    export: "Exporter",
    soonTitle: "Bientôt disponible",
    soonText: "Cette partie de Hearth arrive bientôt.",
  },
  dev: {
    title: "Simulation du lien",
    server: "Serveur",
    state: "État",
    retryDone: "Retour à connecté",
    operation: "Issue d'opération",
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
