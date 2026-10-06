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
| Mise à jour | `tauri-plugin-updater` 2.13 (API Rust seule, `semver`, `url`, `base64`, `async-trait` ; `minisign` en dev) | Officiel ; flux GitHub Releases, signature minisign, installateur NSIS (ADR-0017). Rustls + ring, ni aws-lc ni OpenSSL. | Aucune permission du greffon côté web : seules nos commandes `*_update*` (sans adresse ni chemin). |
| Types IPC | `tauri-specta` `=2.0.0-rc.25`, `specta` `=2.0.0-rc.25`, `specta-typescript` 0.0.12 | Types TypeScript générés depuis les commandes Rust (ADR-0002) : aucune erreur non typée. | Versions candidates, épinglées avec `=` (l'API peut casser à chaque rc). `src/bindings.ts` est versionné ; un test (`tests/bindings.rs`) échoue s'il est périmé. |
| Front | `vue` 3, `vue-router` 5 (historique par hachage), `pinia` 4, Composition API et `<script setup>` | Conventions du hub (ADR-0004). | Vue Router 5 exige Vite 7 ou plus. |
| Build front | `vite` 7, `@vitejs/plugin-vue` 6, `typescript` ~5.9, `vue-tsc` 3 | Pile par défaut du hub. | TypeScript 7 (portage natif) est refusé par `vue-tsc` 3 : rester en 5.9 jusqu'à sa prise en charge. |
| Lint, format | `@biomejs/biome` 2 | Pile par défaut du hub ; un seul outil. | La prise en charge des fichiers `.vue` est partielle : les règles d'imports et de variables inutilisés sont coupées sur `*.vue` (le gabarit est invisible pour Biome), `vue-tsc` couvre le reste. |
| Routeur | `vue-router` 5, historique par hachage | Routes `/welcome`, `/servers/:id/...`, `/settings` ; gardes qui redirigent selon les serveurs (aucun serveur, serveur supprimé, rôle). Le hachage convient à une application servie depuis des fichiers locaux. | Une seule table de routes ; le titre et le rôle requis vivent dans `meta`. |
| Tests navigateur | `@playwright/test` (Chromium, dépendance de dev de `apps/desktop`) | Scénarios de la coquille avec le pont de liaison simulé et captures 1366/1920/2560 (revue UX) ; `npm run e2e`. Trois projets : `dev` (Vite, pont simulé), `prod` (le build livré servi par `vite preview`, pont vide : accueil, aucune erreur de console, aucune simulation) et `prod-e2e` (build de production de test avec page de diagnostic : repli de la frontière d'erreur, coquille, « Réessayer »). Lancés aussi par le job CI `desktop`, après `npm run check:dist`. | Pas de coquille Tauri dans ces tests (le pont Tauri n'y est pas exercé) ; Playwright refuse de cliquer un contrôle `aria-disabled` : `click({ force: true })` pour tester le blocage. Navigateur : `npx playwright install chromium`. |
| Tests front | `vitest` 4, `@vue/test-utils`, `happy-dom`, `@tauri-apps/api/mocks` | Pile par défaut du hub ; pont Tauri simulé par `mockIPC`. | `jsdom` 27 exige Node 22.12 ou plus (`require` d'ES module) : `happy-dom` retenu, Node 22.7 en poste de dev. |
| Polices | `@fontsource/sora`, `@fontsource/instrument-sans`, `@fontsource/dm-mono` (sous-ensemble `latin`) | Polices embarquées dans l'app, aucun appel à un service de polices (CSP stricte). | Seul le sous-ensemble `latin` est livré (accents français, œ) ; un autre alphabet demande d'ajouter son sous-ensemble. |
| Date d'un échantillon | `time` 0.3 (déjà au workspace, `parsing`) | Instant RFC 3339 des mesures du tableau de bord (ADR-0015). | Dépendance normale du client, plus seulement de test. |
| Journal | `tracing-subscriber` (déjà au workspace), `tracing-appender` 0.2 | Fichier tournant `logs/` (quotidien, 7 gardés, 16 Mio au plus au total : les plus anciens, par date de modification, supprimés ; au plafond, une dernière ligne « journal plein » puis écritures abandonnées jusqu'au changement de jour ou à la place libérée) dans le dossier de données, initialisé avant tout ; crochet de panique qui écrit directement dans le fichier (sans abonné `tracing`) et montre une boîte de message si le journal est indisponible, posé même si le journal ne s'ouvre pas ; sans console dans le binaire livré, c'est la seule trace. | Écriture synchrone sous verrou (volume faible) pour qu'une panique soit écrite avant la fin du processus. Niveau fixé par le code (`info` livré, `debug` en développement) : `RUST_LOG` n'est lu qu'en développement. La borne de taille est maison (`tracing-appender` ne borne que le nombre de fichiers). |
| Boîte d'erreur de démarrage | `rfd` 0.17 | Message système avec le chemin du journal quand le démarrage échoue, sans `AppHandle` ni greffon (donc sans permission côté web). | Dépendance de plus ; l'ouverture du dossier des journaux passe par `explorer.exe`, sans greffon `opener`. |
| Tests Rust de la coquille | `tauri` avec la feature `test` (runtime simulé) en dev-dependency | `settings.rs` et `window.rs` testés sans fenêtre réelle ; l'entrée de démarrage est un port (`Autostart`) simulé. | Le runtime simulé ne rend pas la visibilité ni la destruction des fenêtres observables. |
| Textes | module maison `src/i18n/fr.ts` + `t()` | Un fichier typé, clés vérifiées à la compilation, zéro dépendance. | Écart avec `vue-translate` (ADR-0011 du hub) : ce paquet charge ses fichiers de traduction par `fetch` (refusé par la CSP `connect-src` sans origine distante) et vit sur GitHub Packages (`npm ci` exigerait un jeton, y compris en CI et chez les contributeurs d'un dépôt public). À rouvrir si un second langage apparaît, avec un chargement par import statique. |

### Sécurité de la coquille

- CSP de production : `default-src 'none'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self'; connect-src ipc: http://ipc.localhost`. Aucune origine distante, aucun `data:` (`assetsInlineLimit: 0`, polices en fichiers). La CSP de développement ajoute le serveur Vite et ses styles injectés.
- Liste blanche : `build.rs` déclare les seules commandes de l'application (`AppManifest`), `capabilities/default.json` n'accorde que ces permissions `allow-*` à la fenêtre `main`. Aucune permission de greffon n'est donnée au web : tout passe par nos commandes.
- Aucune commande réseau : le réseau vivra dans `hearth-link` (ADR-0002).

### Installateur NSIS

- Cible `nsis` seule, `installMode: currentUser`, WebView2 en `embedBootstrapper`, français sans sélecteur de langue (BR-CLIENT-001/002).
- `installer/hooks.nsh` : contrôle de Windows 10 64 bits et de 50 Mo libres avant toute écriture (BR-CLIENT-014), dans `.onGUIInit` et dans une section masquée placée avant celles du modèle (WebView2, copie) : un refus ne laisse rien sur la machine, mode silencieux compris. Le modèle n'a pas de crochet plus tôt.
- `installer/French.nsh` : remplace le fichier français de Tauri pour tutoyer et reformuler la case de désinstallation.
- Désinstallation : le modèle NSIS de Tauri arrête l'application, retire l'entrée de démarrage et propose la case d'effacement des données ; la page à deux boutons « Garder mes serveurs » / « Tout effacer » de la spec exigerait un modèle NSIS entier sur mesure, trop coûteux à maintenir : la case « Tout effacer : ... » (décochée = tout garder) en tient lieu.
- La case de démarrage avec Windows à l'installation (BR-CLIENT-006) est dans l'installateur depuis HRT-21 (ADR-0026) ; le réglage de l'application (BR-CLIENT-007) lit la même entrée.
- La case « Tout effacer » promet d'effacer aussi les mots de passe mémorisés, mais le modèle ne supprime que des dossiers : quand le coffre Windows arrivera, la désinstallation devra y effacer les identifiants (note dans `installer/French.nsh`).

## Contrat du pont de liaison (interface ⇄ `hearth-link`)

Stabilisé avant la vraie liaison (`apps/desktop/src/link/bridge.ts`) :

- chaque abonnement (`onServersChanged`, `onLinkState`, `onOperation`) rend une **promesse** de désabonnement, comme `listen` de Tauri ;
- `onServersChanged` et `onLinkState` **rejouent l'état courant** à l'abonnement : pas de lecture initiale séparée (`listServers` supprimée), donc aucune fenêtre où un ajout ou une suppression de serveur serait perdu ; l'implémentation réelle s'abonne d'abord aux événements puis envoie l'instantané ;
- un événement `link://state` porte `seq`, numéro de séquence strictement croissant PAR SERVEUR fourni par la liaison : l'interface écarte tout événement dont `seq` n'est pas supérieur au dernier connu (instantané en retard, rejeu), l'horloge murale n'entre pas dans l'ordre ; `since` et les dates ne sont que de l'affichage ;
- le garde du routeur n'attend pas sans fin : la liste des serveurs est abandonnée après 5 s (état d'erreur `loadFailed`, l'accueil s'affiche) ; un abonnement aux liens qui échoue est retenté de plus en plus espacé (1 s à 30 s) et les serveurs sans événement s'affichent « Hors ligne » en attendant ;
- `link://operation` porte `serverId` et `opId` ; le store garde les issues par `opId` (100 au plus, 10 minutes) pour qu'une page retrouve celle de son action.

## Règles d'interface décidées en revue (HRT-09)

- **Une seule source pour « lien ou rôle manquant »** : la prop `needsLink` de `HButton` et `HToggle` (`useNeedsLink`). Pas de directive qui retouche le DOM d'un composant qui lie déjà les mêmes attributs.
- **La coquille n'est jamais dans une frontière d'erreur qui la remplace** ; seule une erreur de rendu d'une page affiche le repli. Le tri « rendu ou non » se fait sur les codes d'erreur exportés par Vue (`ErrorCodes`), valable pour les deux formes de l'`info` donnée à `onErrorCaptured` (libellé en développement, adresse avec code en production : `errors/phase.ts`).
- **Aucune valeur visuelle littérale** hors de `tokens.css`, vérifié par `scripts/check-style.mjs` (dans `npm run lint`). Couverture exacte : couleurs (#hex, `rgb()`, `hsl()`, `hwb()`, `lab()`, `lch()`, `oklab()`, `oklch()`, `color()`, les 148 couleurs nommées dans une propriété de couleur), longueurs (`px`, `rem`, `em`, `pt`, `pc`, `cm`, `mm`, `in`, `ch`, `ex`, `vmin`, `vmax`, `vw`/`vh` hors `100vw`/`100vh`), `opacity` (0 ou 1 seulement), `line-height`, `z-index`, `letter-spacing`, `font-weight`, durées en `s`/`ms` (sauf 0) ; admis sans jeton : pourcentages, `0`, nombres sans unité d'autres propriétés, mots-clés. Aussi : aucun `style=` en ligne, aucun jeton sans utilisateur (nom exact). Les jetons reviennent avec les tickets qui en ont besoin.
- **Aucun code de simulation ni de diagnostic dans le binaire livré**, vérifié par `scripts/check-dist.mjs` (CI). Pour tester la frontière d'erreur sur un build de production, un build de TEST distinct (`vite build --mode e2e`, dossier `dist-e2e`, jamais livré) ajoute une page de diagnostic dont le rendu plante une fois ; la route n'existe que sous `import.meta.env.MODE === "e2e"` (constante de build). Projet Playwright `prod-e2e` sur `vite preview --outDir dist-e2e`.
- Dialogue de confirmation : élément `<dialog>` natif et `showModal()`.

## Comment l'appliquer

- Ajouter une dépendance front : `npm install` dans `apps/desktop`, `package-lock.json` committé, mise à jour de cette table.
- Ajouter une commande Tauri : fonction `#[tauri::command] #[specta::specta]` dans `commands.rs`, ligne dans `collect_commands!` (`lib.rs`), nom dans `COMMANDS` de `build.rs`, permission `allow-...` dans `capabilities/default.json`, puis `HEARTH_REGEN_BINDINGS=1 cargo test -p hearth-desktop` pour régénérer `src/bindings.ts`.
- Erreurs de l'interface : la commande `log_frontend_error(source, message)` les écrit au journal ; message coupé à 2000 caractères, 20 lignes par minute (`domain.rs::FrontendErrorLimiter`).
- Ajouter un texte d'erreur : une nouvelle `kind` de `AppError` ne compile pas côté front sans son entrée dans `src/i18n/index.ts` (`ERROR_KEYS`).
- Passer à Tauri 3 ou à TypeScript 7 : nouvelle ADR.

## Quand NE PAS l'appliquer / limites

- Pas de ressource distante (polices, scripts, images) dans l'interface.
- Pas de code de simulation dans le binaire livré : `SimulatedLinkBridge` et `DevLinkPanel` ne sont atteignables que sous `import.meta.env.DEV` (vérifier `dist/` après un changement de `src/link/index.ts`).
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
