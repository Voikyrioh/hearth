# Architecture — apps/desktop (client Windows)

Stack : Tauri 2 (cœur Rust) · Vue 3 · TypeScript · Vite · Pinia · Biome · Vitest
Style : coquille mince en Rust, interface en Atomic design. Aucun accès réseau ici : le réseau vivra dans `crates/hearth-link` (ADR-0002).
Entrée : `src-tauri/src/main.rs` (→ `lib.rs::run`), `src/main.ts`
Maj : 2026-10-04 (HRT-08)

## Carte

```
apps/desktop/
├── src-tauri/           → Coquille Tauri (crate `hearth-desktop`, membre du workspace Cargo, hors membres par défaut)
│   ├── src/
│   │   ├── lib.rs       → Composition : greffons, commandes typées, fenêtre, zone de notification ; export de `bindings.ts` en debug
│   │   ├── domain.rs    → Règles pures sans E/S ni Tauri : décision de fermeture, entrées du menu, argument `--minimized`, `Settings`
│   │   ├── settings.rs  → Réglages : port `Autostart` (entrée de démarrage Windows, greffon autostart) ; fichier interne `settings.json` (greffon store, explication de fermeture) lu séparément et sans effet sur le réglage de démarrage
│   │   ├── commands.rs  → Commandes exposées à l'interface (`get_settings`, `set_launch_at_startup`, `get_app_version`, `open_logs_folder`)
│   │   ├── logging.rs   → Journal tournant `%APPDATA%/fr.voikyrioh.hearth/logs` (quotidien, 7 fichiers), crochet de panique, ouverture du dossier
│   │   ├── window.rs    → Fenêtre principale : `show_main`, fermeture = masquage, notification d'explication unique
│   │   ├── tray.rs      → Icône de la zone de notification et son menu
│   │   ├── error.rs     → `AppError` (`store`, `autostart`, `logs`), sérialisée `{ kind, message }`
│   │   └── texts.rs     → Textes de la coquille (menu, notification, boîte d'erreur de démarrage)
│   ├── tests/           → Tests d'intégration (règles pures, erreur, `bindings.ts` à jour, journal, réglages et fenêtre avec le runtime simulé de Tauri)
│   ├── capabilities/    → Liste blanche de permissions de la fenêtre `main`
│   ├── installer/       → Crochets NSIS (`hooks.nsh` : contrôles d'avant installation) et textes français (`French.nsh`)
│   ├── icons/           → Icônes générées par `tauri icon` depuis `icons/source/` ; `tray.png` pour la zone de notification
│   ├── build.rs         → Liste blanche des commandes (`AppManifest`), manifeste Windows des tests
│   └── tauri.conf.json  → Fenêtre 1280 × 800 (min 1100 × 680, fond `#1c1518`), CSP stricte, NSIS par utilisateur
├── src/
│   ├── bindings.ts      → GÉNÉRÉ par tauri-specta (ne pas éditer ; `HEARTH_REGEN_BINDINGS=1 cargo test -p hearth-desktop`)
│   ├── i18n/            → `fr.ts` (tous les textes, typés) et `t()`
│   ├── styles/          → `tokens.css` (jetons Braise, source unique), `base.css`, `fonts.css` (polices embarquées)
│   ├── components/      → `atoms/` (HButton, HIcon, HLogo, HToggle), `molecules/` (EmptyState, SettingRow), `organisms/` (ServerRail)
│   ├── pages/           → Welcome (premier lancement), Settings (réglages)
│   ├── stores/          → Pinia : `settings` (appelle les commandes typées)
│   └── router/          → Routes `/` et `/settings`, historique par hachage
└── package.json         → Scripts : dev, build, typecheck, lint, test, tauri
```

## Flux

- **Démarrage** : `lib.rs::run` initialise d'abord le journal, puis branche les greffons (single-instance en premier), crée la fenêtre cachée (fond déjà `#1c1518`), construit l'icône de la zone de notification, puis affiche la fenêtre sauf si l'argument `--minimized` (lancement par Windows) est présent.
- **Fermeture** : `window::on_window_event` annule la fermeture, cache la fenêtre, et à la première fois seulement émet la notification d'explication puis mémorise `closeHintSeen`. « Quitter » (menu de l'icône) appelle `app.exit`.
- **Réglage de démarrage** : interrupteur de `Settings.vue` → store `settings` → `commands.setLaunchAtStartup` → `settings::set_launch_at_startup` → greffon autostart → état relu et renvoyé.

- **Échec de démarrage** : journalisé, boîte de message système avec le chemin du journal, sortie 1. Jamais de sortie silencieuse.
- **Désinstallation et coffre** : le modèle NSIS ne supprime que des dossiers. Quand le coffre Windows (Gestionnaire d'identification) stockera des mots de passe, la désinstallation devra aussi y effacer les identifiants du client si la case « Tout effacer » est cochée (voir `installer/French.nsh`).

## Règles

- Le front ne parle au cœur Rust que par `src/bindings.ts` (commandes typées) ; aucune connexion réseau, aucun `localStorage`.
- Un texte = une entrée de `fr.ts`, tutoiement, pas de tiret cadratin. Une valeur visuelle = un jeton de `tokens.css`. Zéro `style=` en ligne. Un SVG = un composant atome.
- Ajouter une commande : voir ADR-0010 (5 endroits, dont `build.rs` et la capacité).

## Commandes

```sh
cd apps/desktop
npm ci                 # installation
npm run dev            # Vite seul (navigateur, sans pont Tauri : les réglages affichent leur erreur)
npm run tauri dev      # application complète
npm run lint           # Biome
npm run typecheck      # vue-tsc
npm test               # Vitest (pont Tauri simulé par mockIPC)
npm run tauri build    # installateur NSIS : target/release/bundle/nsis/Hearth_<version>_x64-setup.exe
```
