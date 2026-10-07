---
id: ADR-0027
titre: Identité visuelle : logo unique à flamme pleine et pointe ronde, icônes générées depuis des SVG, icône de notification à cinq états, illustrations SVG des écrans vides
type: convention
statut: acceptée (quatre choix PROVISOIRES, voir plus bas)
date: 2026-10-07
portee: projet
remplace: —
liens: [ADR-0002, ADR-0016, ADR-0026, BR-CLIENT-013, BR-RESIL-016, HRT-19]
---

# ADR-0027 : Identité visuelle de Hearth

## Contexte

La recette d'art du 2026-10-07 (hub, `contexts/hearth/art/review-2026-10-07-identite.md`) a refusé le logo à flamme découpée (la bible graphique interdit le détail interne), jugé l'icône illisible à 16 et 24 px, noté que l'icône de notification n'avait que quatre rendus pour cinq états (la couleur seule les distinguait) et qu'aucune illustration d'écran vide n'était intégrée. La bible : sept couleurs (`#1c1518`, `#2a2024`, `#f6ece6`, `#b3a19c`, `#ff7b3d`, `#ff4f7a`, `#5fd0c0`), les mêmes que les jetons de `tokens.css`.

## Choix PROVISOIRES (pris par l'agent principal, le détenteur ne les a pas encore confirmés)

Chacun est réversible en remplaçant un ou quelques fichiers, listés ci-dessous (aucune logique à toucher, sauf le point 4) :

1. **Logo unique** : flamme pleine à deux formes, pointe ronde (`apps/desktop/src-tauri/icons/source/logo.svg`). La flamme découpée disparaît. Le tracé existe en **sept copies**, une seule maîtresse : `logo.svg`. Le favicon (`public/favicon.svg`) en est GÉNÉRÉ par `npm run build:icons`. Les cinq autres sont écrites à la main et gardées par un test (`identity-sync.test.ts` compare leurs attributs `d` à `logo.svg`) : `logo-mono.svg`, `logo-mono-clair.svg`, `logo-mono-sombre.svg`, `app-icon.svg` et le tracé en dur de `HLogo.vue`. Pour changer le logo : modifier `logo.svg`, reporter le tracé dans ces cinq copies, lancer `npm run build:icons` ; le test dit laquelle a été oubliée. `HLogo.test.ts` fixe aussi la géométrie (arc `A14 14` de la pointe) : à réécrire avec le nouveau dessin.
2. **Illustrations des écrans vides** : les redessins SVG (`apps/desktop/src/assets/illustrations/vide-*.svg`), aucune image générée. Pour en changer une : remplacer le fichier (même nom) ; aucun code.
3. **Icône de notification** : l'âtre ne change pas, son contenu dit l'état ; « Hors ligne » en rouge (`--crit`). Pour passer le rouge en gris sourd : modifier la couleur dans `icons/source/tray/tray-hors-ligne.svg`, puis `npm run build:icons`.
4. **Écran vide « Comptes »** : sans illustration. Le choix est une ligne de la table `src/assets/illustrations/screens.ts` (`accounts: null`) : mettre le nom d'une illustration existante (`ILLUSTRATIONS`, `index.ts`), ou déposer un nouveau SVG et l'inscrire dans `index.ts` d'abord. Les pages et tous leurs tests (Vitest et Playwright) lisent cette table : rien d'autre à toucher pour échanger, ajouter ou retirer une illustration. Un NOUVEAU dessin demande en plus sa ligne dans `index.ts` et dans `ILLUSTRATION_FILES` (même fichier `screens.ts`). Même table pour l'accueil, le journal et le hors ligne.

## Décision

1. **Sources = SVG, images = générées, accord vérifié.** Les SVG sont dans `apps/desktop/src-tauri/icons/source/` (logo, monochromes, icône d'application en deux dessins : `app-icon-16.svg` pour 16, 24, 32 px, `app-icon.svg` pour 48 px et plus, icônes de notification dans `tray/`). `npm run build:icons` (`scripts/build-icons.mjs`) produit : `icon.ico` (trames 16, 24, 32, 48, 64, 256, en PNG), les PNG de Tauri, `icons/tray/tray-<état>-<taille>.png` (16, 20, 24, 32) et `installer/sidebar.bmp`. Les images générées sont versionnées : le build Tauri les lit telles quelles. Le script écrit aussi `icons/sources.sha256.json` (SHA-256 de chaque SVG source et de la spécification `icon-spec.mjs`, la version du rendu `RENDER_VERSION`, à monter à la main quand le rendu change, les couleurs du bandeau lues dans `tokens.css`, et pour chaque image PRODUITE son SHA-256 et les sources dont elle dépend) ; `identity-sync.test.ts` recalcule les empreintes et échoue si une source a changé sans régénération, ou si une image a été retouchée ou échangée à la main. L'empreinte d'une image vaut pour le fichier commité : le test ne régénère rien, le rendu de Chromium n'a donc pas à être stable d'une version à l'autre (le script entier n'est volontairement pas empreint : un reformatage ne casse rien). Il contrôle aussi le contenu de chaque image (ni vide, ni unie, couleur de l'état présente) : le décodage PNG est maison (`png-decode.ts`), les octets ne sont comparés qu'à l'empreinte commitée.
2. **Aucune dépendance de plus.** Le rendu se fait dans le Chromium de Playwright (déjà là pour les tests de bout en bout, via un canvas) ; l'assemblage de l'ICO et du BMP est du code maison (une quarantaine de lignes, testé). Écartés : une bibliothèque de rendu SVG native (`resvg`, `sharp`) et `tauri icon` (ne sait pas utiliser deux dessins selon la taille).
3. **Installateur** : même icône (`installerIcon = icons/icon.ico`) et un bandeau latéral aux couleurs de Hearth (`bundle.windows.nsis.sidebarImage = installer/sidebar.bmp`, BMP 24 bits 164 × 314, la taille attendue par NSIS). Le comportement de l'installateur (ADR-0026) ne change pas.
4. **Icône de notification** : six images (cinq états du lien plus « aucun serveur »), `presence::TrayIcon` dit laquelle ; `TrayStatus` et toute la logique de présence (gravité, serveur reflété, notifications) sont inchangés. `TrayPort::show_icon` (appelée avant `show`) pose l'image ; elle n'a pas d'implémentation par défaut : l'adaptateur et chaque double de test l'implémentent, et `tests/alerts.rs` vérifie l'image demandée pour chacun des six états. La taille suit l'échelle de l'écran PRINCIPAL au moment du changement d'état (`tray_icons::pick_size`) : limite connue, une barre des tâches sur un second écran d'une autre échelle, ou un changement d'échelle sans changement d'état, garde l'ancienne taille. La pastille peinte au vol (`badge.rs`) disparaît. « Aucun serveur » : l'âtre seul en trait sourd (`#b3a19c`), le plus sobre (rien n'est dit, rien n'est en panne).
5. **Illustrations** : `EmptyState` prend `illustration` (`firstLaunch`, `journal`, `offline`) et `size` ; image décorative (`alt=""`), taille par jetons (`--illustration-md` 220 px, `--illustration-lg` 280 px), servie comme fichier (CSP `img-src 'self'`). Accueil : `firstLaunch` ; journal vide (sans filtre) : `journal` ; tableau de bord d'un serveur hors ligne dont rien n'est en mémoire : `offline`.
6. **Tests** : `src/assets/identity.test.ts` et `identity-sync.test.ts` (fichiers, trames de l'ICO, dimensions, BMP, états distincts, couleurs des SVG dans la palette, jetons), `tests/tray_icons.rs` (images, états, tailles), Vitest de `HLogo` et `EmptyState`, `e2e/identity.spec.ts` (captures 1366, 1920, 2560).

## Conséquences

- La bible dit « seules couleurs autorisées » ; l'icône de notification utilise aussi les jetons d'état `--ok`, `--warn`, `--crit` : le test de palette les admet pour `tray/` seulement.
- Les couleurs du bandeau viennent de `tokens.css` (`--bg`, `--card`, `--ac`) ; les SVG gardent les leurs, contrôlées contre la palette par `identity.test.ts`.
- Point ouvert : `app-icon-16.svg` (dessin dédié 16, 24, 32 px de la recette) garde sa pointe aiguë, non retouchée.
- Les images générées dépendent de la version de Chromium : régénérer peut changer des octets sans changer le dessin. Les tests vérifient des dimensions, des trames, du contenu et l'empreinte du fichier commité, jamais un nouveau rendu.
- La vérification de l'installateur reste celle de la CI (job `desktop`, `scripts/installer-ci.ps1`) ; rien n'est exécuté en local.
