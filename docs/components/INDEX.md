# Composants — Hearth

Interface Vue 3, atomic design. Aucune logique réseau dans la vue : tout passe par le pont de liaison (`src/link/`, interface `LinkBridge`) et les stores Pinia `servers`, `link`, `toasts`. Une fiche par composant.

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
- [`HSegmented`](./HSegmented.md) — Choix exclusif (radiogroup).

## Molécules

- [`LinkStatePill`](./LinkStatePill.md) — Pastille d'état du lien (5 états).
- [`ServerAvatar`](./ServerAvatar.md) — Avatar de serveur (initiales, anneau, pastille d'état).
- [`StaleStamp`](./StaleStamp.md) — « Vu il y a 12 s », en direct.
- [`StaleSurface`](./StaleSurface.md) — Enveloppe des données périmées (désaturées et datées).
- [`ToastStack`](./ToastStack.md) — Notifications empilées (3 visibles, compteur).
- [`ConfirmDialog`](./ConfirmDialog.md) — Confirmation modale (piège à focus, défaut sûr).
- [`ErrorBoundary`](./ErrorBoundary.md) — Frontière d'erreur d'une page.
- [`EmptyState`](./EmptyState.md) — Placeholder de vue vide.
- [`SettingRow`](./SettingRow.md) — Ligne de réglage.
- [`ComingSoonPanel`](./ComingSoonPanel.md) — Carte « Bientôt disponible ».

## Organismes

- [`ServerRail`](./ServerRail.md) — Barre des serveurs.
- [`ServerNav`](./ServerNav.md) — Navigation du serveur.
- [`AppHeader`](./AppHeader.md) — En-tête avec pastille du lien.
- [`OfflineBanner`](./OfflineBanner.md) — Bandeau hors ligne.
- [`DevLinkPanel`](./DevLinkPanel.md) — Panneau de simulation (développement seulement).

## Gabarits

- [`ServerLayout`](./ServerLayout.md) — Gabarit d'un serveur : navigation, en-tête, bandeau, page.

## Pages

- [`Welcome`](./Welcome.md) — Accueil (aucun serveur).
- [`Dashboard`](./Dashboard.md) — Tableau de bord (à venir).
- [`Accounts`](./Accounts.md) — Comptes (à venir).
- [`Audit`](./Audit.md) — Journal d'activité (à venir).
- [`Settings`](./Settings.md) — Réglages.

## Directive

- [`v-needs-link`](./v-needs-link.md) — désactive et explique toute action qui exige le lien ou un rôle.

## À venir (autres tickets)

`Gauge`, `Sparkline`, `TimeSeriesChart`, `FingerprintBlock`, `PasswordRules`, `AddServerWizard`, `FingerprintAlert`, `MachineCards`, `AccountTable`, `AccountForm`, `AuditTable`, `AuditFilters`, `UpdatePanel`, `AgentUpdateSteps`.

Fichiers de l'interface : `apps/desktop/src/` (`components/{atoms,molecules,organisms}`, `layouts/`, `pages/`, `directives/`, `stores/`, `link/`, `errors/`, `router/`, `i18n/`, `styles/`). Pont Tauri typé : `src/bindings.ts` (généré, ne pas éditer).

## Conventions

- Tous textes → module `src/i18n/fr.ts` + `t(clé, { paramètres })` (clés par chemin, `welcome.title`). Le raccordement à `vue-translate` est repoussé (voir ADR-0010).
- Aucun texte en dur. Aucun `style=` en ligne : une valeur visuelle = un jeton de `src/styles/tokens.css` (la couleur d'un serveur est un numéro de palette).
- Désactivé = `aria-disabled` (jamais `disabled` natif : le focus clavier est conservé). Pour le lien et le rôle : `v-needs-link`, nulle part ailleurs.
- Responsive : captures 1366, 1920 et 2560 px (`npm run e2e`, `apps/desktop/e2e/screenshots/`, non commitées).
