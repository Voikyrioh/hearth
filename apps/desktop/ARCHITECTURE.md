# Architecture — apps/desktop (client Windows)

Stack : Tauri 2 (cœur Rust) · Vue 3 · TypeScript · Vite · Pinia · vue-router · Biome · Vitest · Playwright
Style : coquille mince en Rust, interface en Atomic design. Aucun accès réseau dans l'interface : le réseau est dans `crates/hearth-link`, embarqué par la coquille (ADR-0002, ADR-0013, ADR-0016).
Entrée : `src-tauri/src/main.rs` (→ `lib.rs::run`), `src/main.ts`
Maj : 2026-10-05 (HRT-12)

## Carte

```
apps/desktop/
├── src-tauri/           → Coquille Tauri (crate `hearth-desktop`, membre du workspace Cargo, hors membres par défaut)
│   ├── src/
│   │   ├── lib.rs       → Composition : greffons, commandes typées, fenêtre, zone de notification ; export de `bindings.ts` en debug
│   │   ├── domain.rs    → Règles pures sans E/S ni Tauri : décision de fermeture, entrées du menu, argument `--minimized`, `Settings`, bornes et nettoyage des erreurs de l'interface (`truncate_chars`, `single_line`, `FrontendErrorLimiter`)
│   │   ├── settings.rs  → Réglages : port `Autostart` (entrée de démarrage Windows, greffon autostart) ; fichier interne `settings.json` (greffon store, explication de fermeture) lu séparément et sans effet sur le réglage de démarrage
│   │   ├── commands.rs  → Commandes exposées à l'interface (`get_settings`, `set_launch_at_startup`, `get_app_version`, `open_logs_folder`, `log_frontend_error` : erreur de l'interface écrite au journal, une ligne, bornée en taille et en débit)
│   │   ├── logging.rs   → Journal tournant `%APPDATA%/fr.voikyrioh.hearth/logs` (quotidien, 7 fichiers, 16 Mio au plus), plafond de taille (dernière ligne « journal plein », la rotation quotidienne rouvre, le plus ancien = date de modification), crochet de panique qui écrit directement dans le fichier (boîte de message si le journal est indisponible), ouverture du dossier ; niveau fixé par le code
│   │   ├── link.rs      → Pont réel : `LinkRuntime` enveloppe le `LinkManager` (commandes : sonde, ajout par première connexion `add_and_login`, connexion, déconnexion, modification, suppression, oubli des identifiants, empreinte acceptée) et le relais `forward` qui traduit ses événements en signaux `link://*` ; `StateObserver` (notifications système et icône, branché par `set_observer` qui lui donne l'état courant), `execute` (commande `run_action` : action envoyée à un serveur, refusée hors « Connecté », « résultat inconnu » si le lien tombe avant la réponse) ; `PendingBook` (destination d'événements branchée à l'ouverture de la liaison) tient l'état qu'une interface qui arrive après coup doit relire : alerte d'empreinte en attente, suivis perdus, issues d'actions non lues ; sondes lues par adresse (seule une empreinte lue peut être confirmée) ; écrit dans un `UiSink` (la fenêtre en production), sans dépendre de Tauri
│   │   ├── link_commands.rs → Commandes typées de liaison (liste blanche), `TauriSink`
│   │   ├── link_dto.rs  → Types sérialisés vers l'interface (`ServerDto`, `LinkStateDto`, `LinkFailure` sans texte, charges d'événements), `StateBook` (numéro de séquence par serveur, rejeu de l'état courant), noms des événements
│   │   ├── vault.rs     → Coffre : port `Vault` de `hearth-link` sur le Gestionnaire d'identification de Windows (`Hearth/{id}`, `Hearth/{id}/token`), `CredentialBackend` pour les tests
│   │   ├── window.rs    → Fenêtre principale : `show_main`, fermeture = masquage, notification d'explication unique
│   │   ├── presence.rs  → Présence hors fenêtre, règles PURES : `NotificationGate` (une notification « hors ligne » et une « de retour » au plus par minute et par serveur, retenue agrégée annoncée à l'échéance avec son compte, situation déjà annoncée jamais répétée, BR-RESIL-015), `LinkPresence` (serveur affiché et états : ce que reflète l'icône, BR-RESIL-016), `TrayStatus`, `paint_badge` (pastille verte, orange ou rouge dessinée sur la flamme)
│   │   ├── alerts.rs    → Colle de la présence : `Alerts` (observateur des états, ports `Notifier` et `TrayPort`, horloge injectée, réglage « Notifier quand un serveur devient hors ligne ou revient »)
│   │   ├── tray.rs      → Icône de la zone de notification et son menu ; `TauriTray` (icône + infobulle selon l'état), `TauriNotifier` (greffon de notifications)
│   │   ├── error.rs     → `AppError` (`store`, `autostart`, `logs`), sérialisée `{ kind, message }`
│   │   └── texts.rs     → Textes de la coquille (menu, notification, boîte d'erreur de démarrage)
│   ├── tests/           → Tests d'intégration (règles pures, erreur, `bindings.ts` à jour, journal, réglages et fenêtre avec le runtime simulé de Tauri ; `link_dto.rs`, `vault.rs`, `link_runtime.rs` : la liaison contre un vrai agent et un mandataire à pannes, support partagé avec `hearth-link` ; `presence.rs`, `alerts.rs` : limiteur de notifications, icône ; `offline.rs` : coupures, notifications, icône, action coupée contre un vrai agent, sans assertion de durée)
│   ├── capabilities/    → Liste blanche de permissions de la fenêtre `main`
│   ├── installer/       → Crochets NSIS (`hooks.nsh` : contrôles d'avant installation) et textes français (`French.nsh`)
│   ├── icons/           → Icônes générées par `tauri icon` depuis `icons/source/` ; `tray.png` pour la zone de notification
│   ├── build.rs         → Liste blanche des commandes (`AppManifest`), manifeste Windows des tests
│   └── tauri.conf.json  → Fenêtre 1280 × 800 (min 1100 × 680, fond `#1c1518`), CSP stricte, NSIS par utilisateur
├── src/
│   ├── bindings.ts      → GÉNÉRÉ par tauri-specta (ne pas éditer ; `HEARTH_REGEN_BINDINGS=1 cargo test -p hearth-desktop`)
│   ├── i18n/            → `fr.ts` (tous les textes, typés) et `t(clé, paramètres)`
│   ├── styles/          → `tokens.css` (jetons Braise, source unique), `base.css`, `fonts.css` (polices embarquées)
│   ├── link/            → Pont de liaison : interface `LinkBridge` (abonnements asynchrones qui rejouent l'état courant ; commandes de sonde, ajout, connexion, déconnexion, modification, suppression, oubli, empreinte acceptée), `TauriLinkBridge` (commandes et événements `link://*` de la coquille), `SimulatedLinkBridge` (développement et tests : petit réseau d'agents simulés, empreinte changée pilotable), `NullLinkBridge` (toute commande échoue), `createLinkBridge` (choisit : fenêtre Tauri, navigateur de développement, build livré), `messages` (texte d'un échec typé), `types`
│   ├── stores/          → Pinia : `settings` (commandes typées), `servers`, `link` (états par serveur, issues d'opération par `opId`, alertes d'empreinte, avis de la liaison), `toasts` (alimentés par le pont)
│   ├── composables/     → `useNeedsLink` (la seule règle « lien ou rôle manquant »), `useServerAction` (lancer une action : résultat inconnu à la coupure, notifications discrètes, jamais de rejeu), `useNow` (horloge partagée), `useCurrentServer`, `useAddServer` (les 3 temps de l'assistant), `useEditServer` (modification d'un serveur), `useReconnect` (reconnexion d'un serveur sans session), `useServerBook` (carnet : modifier, supprimer, déconnecter, oublier), `useCountdown` (compte à rebours), `format` (âge, heure, initiales), `arrowNav` (flèches dans une liste)
│   ├── errors/          → `report` (notification + journal du client, borné), `install` (gestionnaires globaux Vue et navigateur)
│   ├── components/      → `atoms/` (HButton, HIcon, HInput, HPasswordInput, HCheckbox, HTag, HTooltip, HSpinner, HSegmented, HToggle, HLogo), `molecules/` (LinkStatePill, ServerAvatar, StaleStamp, StaleSurface, ToastStack, ConfirmDialog, ErrorBoundary, EmptyState, SettingRow, ComingSoonPanel, ColorSwatches, StepTrail, FingerprintBlock, BridgeDownBanner), `organisms/` (ServerRail, ServerNav, AppHeader, OfflineBanner, DevLinkPanel, DevActionPanel, LoginForm, AddServerWizard, FingerprintAlert, ReconnectPanel, ServerEditForm, ServerRow)
│   ├── diagnostics/     → Page de diagnostic du build de test (`--mode e2e`) seulement ; absente du build livré
│   ├── layouts/         → `ServerLayout` (navigation + en-tête + bandeau + page dans sa frontière d'erreur)
│   ├── pages/           → Welcome, AddServer, Servers, Settings, Dashboard, Accounts, Audit (les trois dernières : « Bientôt disponible »)
│   ├── validation/      → `server.ts` : miroir des contrôles de saisie d'un serveur (nom, adresse, port), la règle de référence est `hearth-link domain/book.rs`
│   ├── router/          → Routes et gardes (voir Flux)
│   └── test/            → Aides des tests Vitest (pont simulé, application en mémoire)
├── e2e/                 → Playwright : `shell.spec.ts` et `connect.spec.ts` (interface servie par Vite, pont simulé, captures 1366/1920/2560 dans `e2e/screenshots/`, non commitées) ; `prod.spec.ts` (build livré servi par `vite preview`, pont vide) et `prod-crash.spec.ts` (build de production de test `dist-e2e` : page de diagnostic, frontière d'erreur)
├── scripts/             → `check-style.mjs` (aucune valeur visuelle littérale hors des jetons, aucun jeton inutilisé : couverture exacte dans ADR-0010 ; branché dans `npm run lint`), `check-dist.mjs` (aucun code de simulation ni de diagnostic dans `dist/`)
├── playwright.config.ts → Trois projets : `dev` (port 1420), `prod` (4173, `dist/`), `prod-e2e` (4174, `dist-e2e/`)
└── package.json         → Scripts : dev, build, typecheck, lint (Biome + garde de style), test, e2e, check:dist, tauri
```

## Flux

- **Démarrage** : `lib.rs::run` initialise d'abord le journal, puis branche les greffons (single-instance en premier), crée la fenêtre cachée (fond déjà `#1c1518`), construit l'icône de la zone de notification, puis affiche la fenêtre sauf si l'argument `--minimized` (lancement par Windows) est présent.
- **Fermeture** : `window::on_window_event` annule la fermeture, cache la fenêtre, et à la première fois seulement émet la notification d'explication puis mémorise `closeHintSeen`. « Quitter » (menu de l'icône) appelle `app.exit`.
- **Réglage de démarrage** : interrupteur de `Settings.vue` → store `settings` → `commands.setLaunchAtStartup` → `settings::set_launch_at_startup` → greffon autostart → état relu et renvoyé.

- **Pont de liaison** : l'interface ne parle aux serveurs que par `LinkBridge` (`src/link/`). Contrat : chaque abonnement rend une promesse de désabonnement (comme `listen` de Tauri) ; `onServersChanged` et `onLinkState` rejouent l'état courant à l'abonnement (écoute posée d'abord, instantané lu ensuite : aucune fenêtre où un ajout est perdu) ; un événement `link://state` n'est pris que si son `seq` (numéro croissant par serveur, attribué par la coquille, `StateBook`) dépasse le dernier connu ; la liste des serveurs est abandonnée après 5 s (message à l'écran et « Réessayer ») et un abonnement qui échoue est retenté (« Hors ligne » en attendant, jamais « Connecté » sans écouteur actif). Dans la fenêtre Tauri `createLinkBridge` rend `TauriLinkBridge` (commandes typées et événements `link://*` du `LinkManager`, ADR-0013) ; en navigateur de développement (`npm run dev`) le pont simulé (un agent d'exemple à `192.168.1.50`, compte `marie` / `Correct-Horse-9`, `window.__hearthSim`, panneau `DevLinkPanel`, `?nodev` le masque, `?servers=none` part sans serveur) ; dans le build livré servi hors Tauri, le pont vide. Le code simulé est derrière `import.meta.env.DEV` : absent de `dist/` (`npm run check:dist`).
- **Routes** : `/` (redirige), `/welcome` (aucun serveur), `/servers/new` (assistant d'ajout, toujours permis), `/servers` (carnet, sans serveur retour à l'accueil), `/servers/:id/{dashboard,accounts,audit}` dans `ServerLayout`, `/settings`. Gardes (`router/index.ts::redirectFor`) : aucun serveur vers l'accueil ; serveur inconnu ou supprimé vers le premier serveur, sinon l'accueil ; `accounts` et `audit` fermés au rôle lecture seule ; `router.onError` : notification et journal. Historique par hachage.
- **Gabarit d'un serveur** : `ServerLayout` pose `servers.currentId` d'après la route ; l'en-tête (titre + pastille du lien) et le bandeau hors ligne sont HORS de toute frontière d'erreur, la page est dans `ErrorBoundary`. Les pages mettent leurs actions dans l'en-tête avec `<Teleport defer to="#header-actions">`.
- **Lien ou rôle manquant** : une seule règle, `useNeedsLink`, utilisée par la prop `needsLink` de `HButton` et `HToggle`. L'état final est `disabled || busy || raison du lien` calculé DANS le composant (`aria-disabled`, clic ignoré) et la raison s'affiche par `HTooltip` (survol et focus clavier). Aucune retouche du DOM par un tiers. Voir `docs/components/needs-link.md`.
- **Données périmées** : `StaleSurface` désature et date (`StaleStamp`) tout contenu quand le lien n'est pas « Connecté » ; chaque page enveloppe son contenu (BR-RESIL-007).
- **Hors ligne (HRT-12)** : reconnexion (< 30 s) = pastille « Reconnexion… », rien d'autre ; hors ligne = bandeau, données périmées, actions grisées et expliquées ; aucune fenêtre bloquante (BR-RESIL-011). Session expirée / accès révoqué : `ReconnectPanel` au-dessus de la dernière vue (« Me reconnecter », « Utiliser un autre compte »). Échecs de reconnexion répétés : UNE notification par serveur dont le compteur monte (`toasts.push` à clé). Action coupée : `useServerAction` dit « Le résultat de cette action n'est pas connu. », l'issue arrive par `link://operation`.
- **Notifications système et icône** : `LinkRuntime::relay` passe chaque état à l'observateur `Alerts` ; l'icône suit le serveur affiché (`App.vue` → `setDisplayedServer` → `set_displayed_server`), ou le pire état de tous hors d'une page de serveur ; un minuteur de 5 s libère les notifications retenues par la limite d'une par minute. Réglage : `get/set_notify_on_link_change` (`settings.json`, activé par défaut).
- **Erreurs de l'interface** : `ErrorBoundary` entoure le CONTENU d'une page et n'affiche son repli que pour une erreur de rendu (tri sur les codes de Vue, `errors/phase.ts`, valable en développement et en production) ; les erreurs de gestionnaire d'événement et les promesses rejetées, comme tout ce qu'aucune frontière ne prend (`app.config.errorHandler`, `unhandledrejection`, `error`, `errors/install.ts`), donnent une notification discrète (compteur si répétée) + la commande `log_frontend_error`. Jamais d'écran blanc, la coquille n'est jamais remplacée.
- **Échec de démarrage** : `start_or_report` (appelé par `setup`, qui ne rend jamais d'erreur à Tauri car il en panique) journalise, affiche une boîte de message système avec la raison et le chemin du journal (ou le fait qu'il n'a pas pu être écrit), puis sort avec le code 1. Jamais de sortie silencieuse ni d'application sans fenêtre.
- **Connexion à un serveur** : voir ADR-0013. Assistant `AddServerWizard` (adresse, empreinte, connexion) : `probe_server` ne rend que l'empreinte ; « Confirmer » la garde à l'écran ; le serveur n'est enregistré (carnet, empreinte épinglée, coffre) qu'à la connexion réussie (`add_and_login`) : refus ou abandon : rien ne reste ; application tuée pendant l'écriture : au pire un serveur sans session, jamais un secret orphelin. L'alerte d'empreinte, les suivis perdus et les issues d'actions sont des états relus à l'abonnement (`list_fingerprint_alerts`, `list_link_notices`, `list_unread_operations`, lectures non destructives acquittées par identifiant) : un événement parti avant l'écoute de la fenêtre n'est pas perdu. Mot de passe mémorisé au Gestionnaire d'identification de Windows (`Hearth/{id}`), carnet `servers.json`, `snapshots/` et `operations/` dans `%APPDATA%/fr.voikyrioh.hearth`. Empreinte changée : `FingerprintAlert` (bloquante), bandeau « Voir l'alerte » tant que le lien est suspendu. Serveur sans session : `ReconnectPanel` au-dessus de la dernière vue, recréé pour chaque serveur (`:key`) : la saisie de l'un ne part pas vers l'autre.
- **Désinstallation et coffre** : le modèle NSIS ne supprime que des dossiers. Le coffre Windows stocke les mots de passe (`Hearth/*`) : la désinstallation devra aussi y effacer les identifiants du client si la case « Tout effacer » est cochée (suivi, voir `installer/French.nsh`).

## Règles

- Le front ne parle au cœur Rust que par `src/bindings.ts` (commandes typées) ; aucune connexion réseau, aucun `localStorage`.
- Aucune valeur visuelle littérale hors de `tokens.css` (vérifié par `scripts/check-style.mjs`).
- Un texte = une entrée de `fr.ts`, tutoiement, pas de tiret cadratin. Une valeur visuelle = un jeton de `tokens.css`. Zéro `style=` en ligne. Un SVG = un composant atome.
- Ajouter une commande : voir ADR-0010 (5 endroits, dont `build.rs` et la capacité).

## Commandes

```sh
cd apps/desktop
npm ci                 # installation
npm run dev            # Vite seul (navigateur, pont de liaison simulé ; les réglages affichent leur erreur faute de pont Tauri)
npm run tauri dev      # application complète
npm run lint           # Biome + garde de style (valeurs littérales, jetons inutilisés)
npm run typecheck      # vue-tsc
npm test               # Vitest (pont Tauri simulé par mockIPC, pont de liaison simulé)
npm run check:dist     # après build : aucun code de simulation dans dist/
npx playwright install chromium   # une fois
npm run e2e            # build + build:e2e puis scénarios navigateur (dev, prod, prod-e2e) + captures 1366/1920/2560
npm run tauri build    # installateur NSIS : target/release/bundle/nsis/Hearth_<version>_x64-setup.exe
```
