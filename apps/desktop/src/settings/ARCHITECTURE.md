# Architecture — apps/desktop/src/settings

L'onglet « Réglages » d'un serveur de jeu et « Voir tous les réglages » de l'installation (HRT-109, ADR-0051, `BR-FICHE-037`, `040` à `049`, `051` à `054`). Règles PURES, sans E/S, sans Vue ni Pinia. **Aucune règle propre à un jeu** : libellés, aides, bornes, choix, thèmes, valeurs proposées sont ceux de la fiche (`GET /catalog/{id}`, champs `themes` et `settings`) ; les valeurs voulues, l'état du fichier, les écarts et les effets sont ceux de l'agent (`GET /services/{key}/settings`). Une fiche de plus ne demande aucun code client. L'état (changements en cours, mots de passe compris) vit en mémoire dans `composables/useGameSettings.ts`, jamais dans un magasin sérialisé.

```
settings/
├── model.ts      → `SettingModel` (réglage du catalogue + valeur voulue), `modelsOf` (service), `installModelsOf` (installation), verrous (`frozen`, `imposed`, `gone`, `recreate`), `proposedOf`, `defaultUnverified` — BR-FICHE-037
├── edits.ts      → les changements (`set`, `unset`, `reset`), `effectiveOf`, `isModified`, `resettable`, `pendingEdits`, `editIssues`, listes d'identifiants, `heaviestEffect`, `toSaveRequest` (origine, effet, choix d'écart par défaut « garder le fichier ») — BR-FICHE-041, 044, 048
├── filter.ts     → `plain` (sans accents ni majuscules), `matches`, `shown` (Courants / Tout afficher, Modifiés, recherche), `groupByTheme`, `opensByDefault` — BR-FICHE-043, 045
├── importing.ts  → l'import d'un texte collé : `importRows`, `importChanges`, `toImportRequest` (les lignes reconnues sont reprises du texte par l'agent), `undoneLabels` — BR-FICHE-052
├── messages.ts   → les codes mis en phrases : mentions d'effet (une des quatre), refus d'un enregistrement, défauts de forme, états d'import ; jamais un texte de l'agent, jamais une valeur
└── settings.test.ts → les règles sur la fiche réelle de Terraria (32 réglages, 7 thèmes) et la fiche d'essai
```
