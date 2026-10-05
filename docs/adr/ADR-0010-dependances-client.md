---
id: ADR-0010
titre: Dépendances du client : Tauri et greffons, Vue, Pinia, Vite, Biome, Vitest, tauri-specta, polices embarquées
type: librairie
statut: acceptée
date: 2026-10-04
portee: projet
remplace: —
liens: [ADR-0002, ADR-0009, HRT-08]
---

# ADR-0010 — Dépendances du client

## Contexte

HRT-08 pose la coquille Windows du client (`apps/desktop`). ADR-0002 a choisi Tauri 2 + Vue 3 ; cette ADR fige les dépendances concrètes, leurs versions et leurs limites. Contraintes : pas de ressource distante au runtime, installateur sans droits administrateur, interface sans accès réseau, commandes typées de bout en bout.

## Décision

| Brique | Choix | Pourquoi | Limite |
|---|---|---|---|
| Coquille | `tauri` 2.x (`tray-icon`, `image-png`), `tauri-build` 2.x | Décidé en ADR-0002. Fenêtre, zone de notification, CSP, NSIS. | Windows seul pour l'instant (WebView2). |
| Instance unique | `tauri-plugin-single-instance` 2 | Officiel ; le rappel ramène la fenêtre au premier plan (BR-CLIENT-003). | Aucune. |
| Démarrage avec Windows | `tauri-plugin-autostart` 2 | Officiel ; écrit `HKCU\...\Run` avec `--minimized` (BR-CLIENT-006/007). | La valeur porte le nom de produit « Hearth » (le modèle NSIS retire cette même valeur). |
| Réglages locaux | `tauri-plugin-store` 2 | Officiel ; `settings.json` dans `%APPDATA%\fr.voikyrioh.hearth`. | Dossier nommé d'après l'identifiant, pas `%APPDATA%\Hearth` comme prévu en conception. |
| Notifications | `tauri-plugin-notification` 2 | Officiel ; explication de fermeture (BR-CLIENT-005). | Windows exige un identifiant de notification : en `tauri dev` la notification s'affiche sous une identité de substitution. |
| Types IPC | `tauri-specta` `=2.0.0-rc.25`, `specta` `=2.0.0-rc.25`, `specta-typescript` 0.0.12 | Types TypeScript générés depuis les commandes Rust (ADR-0002) : aucune erreur non typée. | Versions candidates, épinglées avec `=` (l'API peut casser à chaque rc). `src/bindings.ts` est versionné ; un test (`tests/bindings.rs`) échoue s'il est périmé. |
| Front | `vue` 3, `vue-router` 5 (historique par hachage), `pinia` 4, Composition API et `<script setup>` | Conventions du hub (ADR-0004). | Vue Router 5 exige Vite 7 ou plus. |
| Build front | `vite` 7, `@vitejs/plugin-vue` 6, `typescript` ~5.9, `vue-tsc` 3 | Pile par défaut du hub. | TypeScript 7 (portage natif) est refusé par `vue-tsc` 3 : rester en 5.9 jusqu'à sa prise en charge. |
| Lint, format | `@biomejs/biome` 2 | Pile par défaut du hub ; un seul outil. | La prise en charge des fichiers `.vue` est partielle : les règles d'imports et de variables inutilisés sont coupées sur `*.vue` (le gabarit est invisible pour Biome), `vue-tsc` couvre le reste. |
| Tests front | `vitest` 4, `@vue/test-utils`, `happy-dom`, `@tauri-apps/api/mocks` | Pile par défaut du hub ; pont Tauri simulé par `mockIPC`. | `jsdom` 27 exige Node 22.12 ou plus (`require` d'ES module) : `happy-dom` retenu, Node 22.7 en poste de dev. |
| Polices | `@fontsource/sora`, `@fontsource/instrument-sans`, `@fontsource/dm-mono` (sous-ensemble `latin`) | Polices embarquées dans l'app, aucun appel à un service de polices (CSP stricte). | Seul le sous-ensemble `latin` est livré (accents français, œ) ; un autre alphabet demande d'ajouter son sous-ensemble. |
| Journal | `tracing-subscriber` (déjà au workspace), `tracing-appender` 0.2 | Fichier tournant `logs/` (quotidien, 7 gardés, 16 Mio au plus au total : les plus anciens supprimés puis écritures abandonnées) dans le dossier de données, initialisé avant tout ; crochet de panique posé même si le journal ne s'ouvre pas ; sans console dans le binaire livré, c'est la seule trace. | Écriture synchrone sous verrou (volume faible) pour qu'une panique soit écrite avant la fin du processus. Niveau fixé par le code (`info` livré, `debug` en développement) : `RUST_LOG` n'est lu qu'en développement. La borne de taille est maison (`tracing-appender` ne borne que le nombre de fichiers). |
| Boîte d'erreur de démarrage | `rfd` 0.17 | Message système avec le chemin du journal quand le démarrage échoue, sans `AppHandle` ni greffon (donc sans permission côté web). | Dépendance de plus ; l'ouverture du dossier des journaux passe par `explorer.exe`, sans greffon `opener`. |
| Tests Rust de la coquille | `tauri` avec la feature `test` (runtime simulé) en dev-dependency | `settings.rs` et `window.rs` testés sans fenêtre réelle ; l'entrée de démarrage est un port (`Autostart`) simulé. | Le runtime simulé ne rend pas la visibilité ni la destruction des fenêtres observables. |
| Textes | module maison `src/i18n/fr.ts` + `t()` | Un fichier typé, clés vérifiées à la compilation, zéro dépendance. | Écart avec `vue-translate` (ADR-0011 du hub) : ce paquet charge ses fichiers de traduction par `fetch` (refusé par la CSP `connect-src` sans origine distante) et vit sur GitHub Packages (`npm ci` exigerait un jeton, y compris en CI et chez les contributeurs d'un dépôt public). À rouvrir si un second langage apparaît, avec un chargement par import statique. |

### Sécurité de la coquille

- CSP de production : `default-src 'none'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self'; connect-src ipc: http://ipc.localhost`. Aucune origine distante, aucun `data:` (`assetsInlineLimit: 0`, polices en fichiers). La CSP de développement ajoute le serveur Vite et ses styles injectés.
- Liste blanche : `build.rs` déclare les seules commandes de l'application (`AppManifest`), `capabilities/default.json` n'accorde que ces trois permissions `allow-*` à la fenêtre `main`. Aucune permission de greffon n'est donnée au web : tout passe par nos commandes.
- Aucune commande réseau : le réseau vivra dans `hearth-link` (ADR-0002).

### Installateur NSIS

- Cible `nsis` seule, `installMode: currentUser`, WebView2 en `embedBootstrapper`, français sans sélecteur de langue (BR-CLIENT-001/002).
- `installer/hooks.nsh` : contrôle de Windows 10 64 bits et de 50 Mo libres avant toute écriture (BR-CLIENT-014), dans `.onGUIInit` et dans une section masquée placée avant celles du modèle (WebView2, copie) : un refus ne laisse rien sur la machine, mode silencieux compris. Le modèle n'a pas de crochet plus tôt.
- `installer/French.nsh` : remplace le fichier français de Tauri pour tutoyer et reformuler la case de désinstallation.
- Désinstallation : le modèle NSIS de Tauri arrête l'application, retire l'entrée de démarrage et propose la case d'effacement des données ; la page à deux boutons « Garder mes serveurs » / « Tout effacer » de la spec exigerait un modèle NSIS entier sur mesure, trop coûteux à maintenir : la case « Tout effacer : ... » (décochée = tout garder) en tient lieu.
- La case de démarrage avec Windows à l'installation (BR-CLIENT-006) n'est pas dans l'installateur : le réglage existe dans l'application (BR-CLIENT-007). Suivi : ticket HRT-21 (page d'options NSIS sur mesure).
- La case « Tout effacer » promet d'effacer aussi les mots de passe mémorisés, mais le modèle ne supprime que des dossiers : quand le coffre Windows arrivera, la désinstallation devra y effacer les identifiants (note dans `installer/French.nsh`).

## Comment l'appliquer

- Ajouter une dépendance front : `npm install` dans `apps/desktop`, `package-lock.json` committé, mise à jour de cette table.
- Ajouter une commande Tauri : fonction `#[tauri::command] #[specta::specta]` dans `commands.rs`, ligne dans `collect_commands!` (`lib.rs`), nom dans `COMMANDS` de `build.rs`, permission `allow-...` dans `capabilities/default.json`, puis `HEARTH_REGEN_BINDINGS=1 cargo test -p hearth-desktop` pour régénérer `src/bindings.ts`.
- Ajouter un texte d'erreur : une nouvelle `kind` de `AppError` ne compile pas côté front sans son entrée dans `src/i18n/index.ts` (`ERROR_KEYS`).
- Passer à Tauri 3 ou à TypeScript 7 : nouvelle ADR.

## Quand NE PAS l'appliquer / limites

- Pas de ressource distante (polices, scripts, images) dans l'interface.
- Pas d'accès direct de la vue web aux greffons (`invoke('plugin:...')`) : exposer une commande de l'application à la place.
- Le build Tauri n'est pas pris en charge sous Linux ici : la coquille est hors des membres par défaut du workspace (voir `CLAUDE.md`).

## Alternatives rejetées

- **Polices Google Fonts au runtime** : ressource distante, interdit par la CSP et par la vie privée.
- **`jsdom`** : incompatible avec Node 22.7 (voir ci-dessus).
- **Modèle NSIS entièrement sur mesure** (page de désinstallation à deux boutons) : maintenance disproportionnée face à la case du modèle.
- **Magasin de réglages dans le front (`localStorage`)** : le cœur Rust est la source de vérité, le front ne persiste rien.

## Conséquences

- Les tests Rust de la coquille vivent dans `apps/desktop/src-tauri/tests/` (tests d'intégration) : `build.rs` embarque le manifeste « Common Controls v6 » dans ces seuls binaires, sinon ils échouent au chargement sous Windows (`STATUS_ENTRYPOINT_NOT_FOUND`). Les tests unitaires de la bibliothèque sont désactivés (`test = false`) pour la même raison. Le journal et son crochet de panique étant globaux au processus, `tests/logging.rs` les initialise dans un seul test.
- Le job CI `desktop` tourne sous Windows, construit aussi l'installateur NSIS et le publie en artefact (`hearth-windows-installer`) ; le job Linux exclut `hearth-desktop`.
- `cargo test --workspace` sous Windows exige que le front ait été construit une fois (`dist/` est lu à la compilation) ; sans lui : `--exclude hearth-desktop`.

## Références

- ADR-0002 (Tauri + Vue), ADR-0004 du hub (conventions Vue), ADR-0009 (dépendances de l'agent).
- Tauri 2 : https://tauri.app · NSIS : https://tauri.app/distribute/windows-installer/
- tauri-specta : https://github.com/specta-rs/tauri-specta
- Fontsource : https://fontsource.org
