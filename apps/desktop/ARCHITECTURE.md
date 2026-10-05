# Architecture — apps/desktop (client Windows)

Stack : Tauri 2 (cœur Rust) · Vue 3 · TypeScript · Vite · Pinia · Biome · Vitest
Style : coquille mince en Rust, interface en Atomic design. Aucun accès réseau ici : le réseau vivra dans `crates/hearth-link` (ADR-0002).
Entrée : `src-tauri/src/main.rs` (→ `lib.rs::run`), `src/main.ts`
Maj : 2026-10-05 (HRT-09)

## Carte

```
apps/desktop/
├── src-tauri/           → Coquille Tauri (crate `hearth-desktop`, membre du workspace Cargo, hors membres par défaut)
│   ├── src/
│   ├── bindings.ts      → GÉNÉRÉ par tauri-specta (ne pas éditer ; `HEARTH_REGEN_BINDINGS=1 cargo test -p hearth-desktop`)
│   ├── i18n/            → `fr.ts` (tous les textes, typés) et `t(clé, paramètres)`
│   ├── styles/          → `tokens.css` (jetons Braise, source unique), `base.css`, `fonts.css` (polices embarquées)
│   ├── link/            → Pont de liaison : interface `LinkBridge` (serveurs, `link://state`, « réessayer maintenant », `link://operation`), `SimulatedLinkBridge` (développement et tests), `NullLinkBridge`, `createLinkBridge` (point de branchement du pont Tauri réel)
│   ├── stores/          → Pinia : `settings` (commandes typées), `servers`, `link`, `toasts` (alimentés par le pont)
│   ├── composables/     → `useNow` (horloge partagée), `useCurrentServer`, `format` (âge, heure, initiales), `arrowNav` (flèches dans une liste)
│   ├── directives/      → `needsLink` (`v-needs-link`)
│   ├── errors/          → `report` (notification + journal du client, borné), `install` (gestionnaires globaux Vue et navigateur)
│   ├── components/      → `atoms/` (HButton, HIcon, HInput, HPasswordInput, HCheckbox, HTag, HTooltip, HSpinner, HSegmented, HToggle, HLogo), `molecules/` (LinkStatePill, ServerAvatar, StaleStamp, StaleSurface, ToastStack, ConfirmDialog, ErrorBoundary, EmptyState, SettingRow, ComingSoonPanel), `organisms/` (ServerRail, ServerNav, AppHeader, OfflineBanner, DevLinkPanel)
│   ├── layouts/         → `ServerLayout` (navigation + en-tête + bandeau + page dans sa frontière d'erreur)
│   ├── pages/           → Welcome, Settings, Dashboard, Accounts, Audit (les trois dernières : « Bientôt disponible »)
│   ├── router/          → Routes et gardes (voir Flux)
│   └── test/            → Aides des tests Vitest (pont simulé, application en mémoire)
├── e2e/                 → Scénarios Playwright sur l'interface servie par Vite avec le pont simulé ; captures dans `e2e/screenshots/` (non commitées)
├── playwright.config.ts → Chromium, port 1420 (celui de Vite)
└── package.json         → Scripts : dev, build, typecheck, lint, test, e2e, tauri
```

## Flux

- **Démarrage** : `lib.rs::run` initialise d'abord le journal, puis branche les greffons (single-instance en premier), crée la fenêtre cachée (fond déjà `#1c1518`), construit l'icône de la zone de notification, puis affiche la fenêtre sauf si l'argument `--minimized` (lancement par Windows) est présent.
- **Fermeture** : `window::on_window_event` annule la fermeture, cache la fenêtre, et à la première fois seulement émet la notification d'explication puis mémorise `closeHintSeen`. « Quitter » (menu de l'icône) appelle `app.exit`.
- **Réglage de démarrage** : interrupteur de `Settings.vue` → store `settings` → `commands.setLaunchAtStartup` → `settings::set_launch_at_startup` → greffon autostart → état relu et renvoyé.

- **Pont de liaison** : l'interface ne parle aux serveurs que par `LinkBridge` (`src/link/`). En navigateur de développement (`npm run dev`), `createLinkBridge` rend le pont simulé (deux serveurs d'exemple, `window.__hearthSim`, panneau `DevLinkPanel`, `?nodev` le masque, `?servers=none` part sans serveur) ; dans la fenêtre Tauri et dans le binaire livré il rend le pont vide : le pont réel sera branché dans `createLinkBridge` par le ticket qui fusionne `hearth-link`. Le code simulé est derrière `import.meta.env.DEV` : absent de `dist/`.
- **Routes** : `/` (redirige), `/welcome` (aucun serveur), `/servers/:id/{dashboard,accounts,audit}` dans `ServerLayout`, `/settings`. Gardes (`router/index.ts::redirectFor`) : aucun serveur vers l'accueil ; serveur inconnu ou supprimé vers le premier serveur, sinon l'accueil ; `accounts` et `audit` fermés au rôle lecture seule. Historique par hachage.
- **Gabarit d'un serveur** : `ServerLayout` pose `servers.currentId` d'après la route ; l'en-tête (titre + pastille du lien) est hors de la frontière d'erreur, le bandeau hors ligne s'affiche si l'état est « Hors ligne », la page est dans `ErrorBoundary`. Les pages mettent leurs actions dans l'en-tête avec `<Teleport defer to="#header-actions">`.
- **`v-needs-link`** : unique mécanisme qui désactive et explique une action selon le lien du serveur courant et le rôle (`aria-disabled`, clic bloqué en capture, `title`). Voir `docs/components/v-needs-link.md`.
- **Données périmées** : `StaleSurface` désature et date (`StaleStamp`) tout contenu quand le lien n'est pas « Connecté ».
- **Erreurs de l'interface** : `app.config.errorHandler`, `unhandledrejection` et `error` (`errors/install.ts`) puis `ErrorBoundary` autour des pages : notification discrète (compteur si répétée) + commande `log_frontend_error`. Jamais d'écran blanc.
- **Échec de démarrage** : `start_or_report` (appelé par `setup`, qui ne rend jamais d'erreur à Tauri car il en panique) journalise, affiche une boîte de message système avec la raison et le chemin du journal (ou le fait qu'il n'a pas pu être écrit), puis sort avec le code 1. Jamais de sortie silencieuse ni d'application sans fenêtre.
- **Désinstallation et coffre** : le modèle NSIS ne supprime que des dossiers. Quand le coffre Windows (Gestionnaire d'identification) stockera des mots de passe, la désinstallation devra aussi y effacer les identifiants du client si la case « Tout effacer » est cochée (voir `installer/French.nsh`).

## Règles

- Le front ne parle au cœur Rust que par `src/bindings.ts` (commandes typées) ; aucune connexion réseau, aucun `localStorage`.
- Un texte = une entrée de `fr.ts`, tutoiement, pas de tiret cadratin. Une valeur visuelle = un jeton de `tokens.css`. Zéro `style=` en ligne. Un SVG = un composant atome.
- Ajouter une commande : voir ADR-0010 (5 endroits, dont `build.rs` et la capacité).

## Commandes

```sh
cd apps/desktop
npm ci                 # installation
npm run dev            # Vite seul (navigateur, pont de liaison simulé ; les réglages affichent leur erreur faute de pont Tauri)
npm run tauri dev      # application complète
npm run lint           # Biome
npm run typecheck      # vue-tsc
npm test               # Vitest (pont Tauri simulé par mockIPC, pont de liaison simulé)
npx playwright install chromium   # une fois
npm run e2e            # scénarios navigateur + captures 1366/1920/2560 (e2e/screenshots/)
npm run tauri build    # installateur NSIS : target/release/bundle/nsis/Hearth_<version>_x64-setup.exe
```
