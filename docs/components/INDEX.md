# Composants — Hearth

Interface Vue 3, atomic design. Aucune logique réseau dans la vue ; tout reçu par événements Tauri typés. Composants partagent l'état Pinia (stores link, settings, servers).

## Atoms

- `HButton` — Bouton actionnable (primary, secondary, danger, disabled).
- `HInput` — Champ texte avec validation de format.
- `HPasswordInput` — Champ mot de passe (affiche/masque, règles visualisées).
- `HBadge` — Libellé statut (color, taille).
- `HIcon` — Icône SVG (role sémantique).
- `HTooltip` — Bulle d'explication (position, délai).
- `HSpinner` — Indicateur chargement (taille).
- `HToggle` — Interrupteur booléen (label, disabled).
- `HSegmented` — Groupe boutons mutuellement exclusifs (options[]).

## Molecules

- `LinkStatePill` — Affiche état du lien (Connected, Reconnecting, Offline, SessionExpired, AccessRevoked ; styles + texte).
- `ServerAvatar` — Boîte avec couleur serveur + icône (clickable).
- `Gauge` — Jauge circulaire (value 0-100, seuil warn/alert, label).
- `Sparkline` — Courbe mini historique, pas d'axes.
- `TimeSeriesChart` — Courbe uPlot axes + legend (responsive, 1 Hz).
- `StaleStamp` — « Vu il y a 2 min » (color warn si > 5 min).
- `ToastStack` — Notifications empilées (dédoublonnage par compteur auto-dismiss).
- `FingerprintBlock` — Affiche empreinte 8 blocs 4 hex majuscules (monospace, copyable).
- `PasswordRules` — Affiche règles mot de passe (checklist, couleur per-rule).
- `ConfirmDialog` — Modal confirmation action destructrice (title, message, boutons).
- `EmptyState` — Placeholder quand liste vide (icône, texte, CTA optionnelle).

## Organisms

- `ServerRail` — Rail gauche : liste serveurs avec avatar + nom, état lien badge.
- `ServerNav` — Onglets serveur sélectionné (Dashboard, Comptes, Audit, Réglages).
- `AppHeader` — Barre haut : logo Hearth, info serveur connecté, bouton paramètres.
- `OfflineBanner` — Banneau alerte haut si lien pas Connected (raison, bouton relancer).
- `AddServerWizard` — Modal multi-étapes (adresse → probe → empreinte → login).
- `FingerprintAlert` — Modal alerte blocante si empreinte change (explication, bouton retirer serveur).
- `MachineCards` — Cartes par section système (CPU, mémoire, disques, réseau, GPU).
- `AccountTable` — Tableau comptes (colonnes : username, rôle, session ouvertes, dernier login ; actions edit/delete si admin).
- `AccountForm` — Formulaire création/édition compte (username, password + règles, select rôle).
- `AuditTable` — Tableau audit (colonnes : heure, acteur, action, cible, résultat, raison ; paginé curseur).
- `AuditFilters` — Contrôles filtres audit (account, action, outcome, date range, recherche texte).
- `UpdatePanel` — Info statut mise à jour agent (version courant, en cours, dernier résultat ; bouton lancer).
- `AgentUpdateSteps` — Timeline étapes mise à jour (télécharge, vérifie, arrête, échange, redémarre, vérif superviseur).

## Pages

- `Welcome` — Page accueil (aucun serveur) : présentation, bouton « Ajouter serveur » → AddServerWizard.
- `Dashboard` — Page tableau de bord (sélection serveur + ServerNav affiche Dashboard).
- `Accounts` — Page comptes (ServerNav + Accounts tab ; table + form création).
- `Audit` — Page audit (ServerNav + Audit tab ; filters + table pagination).
- `Settings` — Page réglages (ServerNav + Settings tab ; langue, log path, à propos).

## Directive v-needs-link

Appliquée à boutons/inputs pour désactiver si :
- Lien pas Connected (toute raison : offline, expired, revoked, reconnecting).
- Rôle insuffisant (BR-RESIL-008).

Exemple :
```vue
<HButton v-needs-link="linkState" @click="deleteAccount">Supprimer</HButton>
```

Affiche infobulle explicative du blocage.

## Conventions

- Tous textes → `vue-translate` (clés `pages.dashboard.title`, …).
- Aucun texte en dur.
- Styles via tokens CSS (colors, spacing, fonts) dans `:root`.
- Responsive : 1920 px + 2560 px pour captures Nora.
