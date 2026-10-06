# Composants — Hearth

Interface Vue 3, atomic design. Aucune logique réseau dans la vue : tout passe par le pont de liaison (`src/link/`, interface `LinkBridge`) et les stores Pinia `servers`, `link`, `toasts`, `dashboard`. Une fiche par composant.

## Atomes

- [`HButton`](./HButton.md) — Bouton (principal, secondaire, destructeur, icône ; tailles ; occupé).
- [`HInput`](./HInput.md) — Champ texte (libellé, aide, erreur, focus braise).
- [`HPasswordInput`](./HPasswordInput.md) — Champ mot de passe (afficher/masquer).
- [`HCheckbox`](./HCheckbox.md) — Case à cocher.
- [`HTag`](./HTag.md) — Étiquette courte.
- [`HIcon`](./HIcon.md) — Jeu d'icônes SVG.
- [`HLogo`](./HLogo.md) — Logo Hearth.
- [`HTooltip`](./HTooltip.md) — Bulle d'explication (survol, focus).
- [`HSpinner`](./HSpinner.md) — Indicateur d'attente.
- [`HToggle`](./HToggle.md) — Interrupteur booléen.
- [`HSelect`](./HSelect.md) — Liste déroulante native (un choix).
- [`HSegmented`](./HSegmented.md) — Choix exclusif (radiogroup).
- [`HGaugeArc`](./HGaugeArc.md) — Arc de jauge de 270° (SVG).
- [`HAreaChart`](./HAreaChart.md) — Courbe pleine à une ou deux séries (SVG), trous pour les mesures absentes.
- [`HBars`](./HBars.md) — Barres verticales (SVG), une par cœur.
- [`HMeter`](./HMeter.md) — Jauge linéaire (SVG), occupation d'un disque.
- [`HSelect`](./HSelect.md) — Liste déroulante native (libellé, masquable).

## Molécules

- [`LinkStatePill`](./LinkStatePill.md) — Pastille d'état du lien (5 états).
- [`ServerAvatar`](./ServerAvatar.md) — Avatar de serveur (initiales, anneau, pastille d'état).
- [`MultiSelect`](./MultiSelect.md) — Liste déroulante à choix multiple (cases à cocher).
- [`AuditDetailDialog`](./AuditDetailDialog.md) — Une entrée du journal en entier.
- [`StaleStamp`](./StaleStamp.md) — « Vu il y a 12 s », en direct.
- [`StaleSurface`](./StaleSurface.md) — Enveloppe des données périmées (désaturées et datées).
- [`ToastStack`](./ToastStack.md) — Notifications empilées (3 visibles, compteur).
- [`ConfirmDialog`](./ConfirmDialog.md) — Confirmation modale (piège à focus, défaut sûr).
- [`FormDialog`](./FormDialog.md) — Fenêtre de formulaire (champs figés pendant l'envoi, erreur dans la fenêtre).
- [`PasswordRules`](./PasswordRules.md) — Critères du mot de passe en direct (coche ou croix et texte).
- [`ReleaseNotesDialog`](./ReleaseNotesDialog.md) — Notes de version en texte brut (boîte modale).
- [`ErrorBoundary`](./ErrorBoundary.md) — Frontière d'erreur d'une page.
- [`EmptyState`](./EmptyState.md) — Placeholder de vue vide.
- [`SettingRow`](./SettingRow.md) — Ligne de réglage.
- [`ComingSoonPanel`](./ComingSoonPanel.md) — Carte « Bientôt disponible ».
- [`ColorSwatches`](./ColorSwatches.md) — Palette des 8 couleurs de serveur.
- [`StepTrail`](./StepTrail.md) — Fil des étapes de l'assistant.
- [`FingerprintBlock`](./FingerprintBlock.md) — Empreinte en 8 groupes de 4.
- [`BridgeDownBanner`](./BridgeDownBanner.md) — Liste des serveurs illisible, avec « Réessayer ».
- [`LevelBadge`](./LevelBadge.md) — Marque d'alerte : icône + « Attention » ou « Critique ».
- [`Gauge`](./Gauge.md) — Jauge : arc, valeur, libellé, marque d'alerte.
- [`TimeSeriesChart`](./TimeSeriesChart.md) — Courbe sur la fenêtre choisie, « Depuis N min » si l'historique est court.
- [`StatRow`](./StatRow.md) — Ligne « libellé : valeur » d'une carte.
- [`DashCard`](./DashCard.md) — Carte du tableau de bord (titre de section, corps).
- [`CoreBars`](./CoreBars.md) — Une barre par cœur du processeur.

## Organismes

- [`AuditFilters`](./AuditFilters.md) — Filtres du journal (recherche, comptes, types, résultats, période).
- [`AuditTable`](./AuditTable.md) — Tableau du journal : grille accessible, virtualisée, rafales regroupées.
- [`ServerRail`](./ServerRail.md) — Barre des serveurs.
- [`ServerNav`](./ServerNav.md) — Navigation du serveur.
- [`AppHeader`](./AppHeader.md) — En-tête avec pastille du lien.
- [`OfflineBanner`](./OfflineBanner.md) — Bandeau hors ligne.
- [`UpdateBanner`](./UpdateBanner.md) — Bandeau de mise à jour du client (annonce, téléchargement, installation, échec).
- [`UpdatePanel`](./UpdatePanel.md) — Section « Mises à jour » des réglages.
- [`LoginForm`](./LoginForm.md) — Formulaire de connexion (identifiant, mot de passe, se souvenir).
- [`AddServerWizard`](./AddServerWizard.md) — Assistant d'ajout en 3 temps.
- [`FingerprintAlert`](./FingerprintAlert.md) — Alerte bloquante d'identité changée.
- [`ReconnectPanel`](./ReconnectPanel.md) — Formulaire de reconnexion d'un serveur sans session.
- [`ServerEditForm`](./ServerEditForm.md) — Modification d'un serveur (nom, couleur, adresse).
- [`ServerRow`](./ServerRow.md) — Ligne du carnet de serveurs.
- [`DevLinkPanel`](./DevLinkPanel.md) — Panneau de simulation (développement seulement).
- [`DevActionPanel`](./DevActionPanel.md) — Bouton d'action de développement (`useServerAction`, `needs-link`).
- [`AccountTable`](./AccountTable.md) — Tableau des comptes et actions de ligne.
- [`CreateAccountDialog`](./CreateAccountDialog.md) — Création d'un compte (validation en direct).
- [`PasswordDialog`](./PasswordDialog.md) — Changement de mot de passe (le sien ou celui d'un autre compte).
- [`OwnAccountCard`](./OwnAccountCard.md) — « Mon compte » d'un serveur dans les réglages.
- [`MachineCard`](./MachineCard.md) — Sections Machine et Durée de fonctionnement.
- [`CpuCard`](./CpuCard.md) — Section Processeur (jauge, courbe, cœurs).
- [`MemoryCard`](./MemoryCard.md) — Section Mémoire.
- [`GpuCard`](./GpuCard.md) — Section Carte graphique (ou « Non disponible sur cette machine »).
- [`GpuPanel`](./GpuPanel.md) — Une carte graphique (charge, mémoire vidéo, température).
- [`NetworkCard`](./NetworkCard.md) — Section Réseau (montant, descendant).
- [`DisksCard`](./DisksCard.md) — Section Disques (suit les montages).
- [`TemperaturesCard`](./TemperaturesCard.md) — Section Températures (ou explication sans sonde).

## Gabarits

- [`ServerLayout`](./ServerLayout.md) — Gabarit d'un serveur : navigation, en-tête, bandeau, page.

## Pages

- [`Welcome`](./Welcome.md) — Accueil (aucun serveur).
- [`AddServer`](./AddServer.md) — Ajout d'un serveur (assistant).
- [`Servers`](./Servers.md) — Carnet de serveurs.
- [`Dashboard`](./Dashboard.md) — Tableau de bord : la machine en direct (jauges, courbes, seuils, matériel absent).
- [`Accounts`](./Accounts.md) — Comptes (administrateurs) : liste, création, rôle, mots de passe, sessions, suppression.
- [`Audit`](./Audit.md) — Journal d'activité (à venir).
- [`Settings`](./Settings.md) — Réglages.

## Règle partagée

- [`needs-link`](./needs-link.md) — prop `needsLink` et `useNeedsLink` : désactive et explique toute action qui exige le lien ou un rôle (unique source de vérité).

## À venir (autres tickets)

`Sparkline` (non retenu : les courbes de chaque carte suffisent), `AuditTable`, `AuditFilters`, `AgentUpdateSteps`.

Fichiers de l'interface : `apps/desktop/src/` (`components/{atoms,molecules,organisms}`, `layouts/`, `pages/`, `composables/`, `stores/`, `link/`, `errors/`, `router/`, `i18n/`, `styles/`). Pont Tauri typé : `src/bindings.ts` (généré, ne pas éditer).

## Conventions

- Tous textes → module `src/i18n/fr.ts` + `t(clé, { paramètres })` (clés par chemin, `welcome.title`). Le raccordement à `vue-translate` est repoussé (voir ADR-0010).
- Aucun texte en dur. Aucun `style=` en ligne : une valeur visuelle = un jeton de `src/styles/tokens.css` (la couleur d'un serveur est un numéro de palette).
- Désactivé = `aria-disabled` (jamais `disabled` natif : le focus clavier est conservé), explication par `HTooltip`. Pour le lien et le rôle : la prop `needsLink`, nulle part ailleurs.
- Aucune valeur visuelle littérale : `npm run lint` échoue sur une couleur ou un `px` hors de `tokens.css`.
- Responsive : captures 1366, 1920 et 2560 px (`npm run e2e`, `apps/desktop/e2e/screenshots/`, non commitées).
