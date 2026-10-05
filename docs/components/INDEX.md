# Composants — Hearth

Interface Vue 3, atomic design. Aucune logique réseau dans la vue ; tout reçu par événements Tauri typés. Composants partagent l'état Pinia (stores link, settings, servers).

## Atoms

- [`HButton`](./HButton.md) — Bouton actionnable (primary, secondary, danger, disabled). **Livré (HRT-08).**
- `HInput` — Champ texte avec validation de format.
- `HPasswordInput` — Champ mot de passe (affiche/masque, règles visualisées).
- `HBadge` — Libellé statut (color, taille).
- [`HIcon`](./HIcon.md) — Icône SVG (role sémantique). **Livré (HRT-08).**
- [`HLogo`](./HLogo.md) — Logo Hearth (flamme dans un âtre), tailles sm/md/lg, version simplifiée en petit. **Livré (HRT-08).**
- `HTooltip` — Bulle d'explication (position, délai).
- `HSpinner` — Indicateur chargement (taille).
- [`HToggle`](./HToggle.md) — Interrupteur booléen (label, disabled). **Livré (HRT-08).**
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
- [`EmptyState`](./EmptyState.md) — Placeholder quand liste vide (icône, texte, CTA optionnelle). **Livré (HRT-08).**
- [`SettingRow`](./SettingRow.md) — Ligne de réglage : libellé, aide, contrôle nommé par `aria-labelledby`. **Livré (HRT-08).**

## Organisms

- [`ServerRail`](./ServerRail.md) — Rail gauche : liste serveurs avec avatar + nom, état lien badge. **Livré (HRT-08).**
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

- [`Welcome`](./Welcome.md) — Page accueil (aucun serveur) : présentation, bouton « Ajouter serveur » → AddServerWizard. **Livré (HRT-08).**
- `Dashboard` — Page tableau de bord (sélection serveur + ServerNav affiche Dashboard).
- `Accounts` — Page comptes (ServerNav + Accounts tab ; table + form création).
- `Audit` — Page audit (ServerNav + Audit tab ; filters + table pagination).
- [`Settings`](./Settings.md) — Page réglages (ServerNav + Settings tab ; langue, log path, à propos). **Livré (HRT-08).**

Fichiers de l'interface : `apps/desktop/src/` (`components/{atoms,molecules,organisms}`, `pages/`, `stores/`, `router/`, `i18n/`, `styles/`). Pont Tauri typé : `src/bindings.ts` (généré, ne pas éditer).

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

- Tous textes → module `src/i18n/fr.ts` + `t(clé)` (clés par chemin, `welcome.title`). Le raccordement à `vue-translate` est repoussé (voir ADR-0010).
- Aucun texte en dur.
- Styles via jetons CSS : `src/styles/tokens.css` est l'unique source (couleurs, espacements, rayons, polices, mouvement) ; zéro `style=` en ligne.
- Responsive : 1920 px + 2560 px pour captures Nora.
