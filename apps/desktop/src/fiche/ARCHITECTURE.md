# Architecture — apps/desktop/src/fiche

Le Catalogue et l'installation d'un serveur de jeu depuis une fiche (HRT-102, ADR-0050, `BR-FICHE-*`). Règles PURES, sans E/S, sans Vue ni Pinia : ce dossier ne parle ni au pont (`link/`) ni à un magasin ; il ne reçoit que des valeurs lues dans la fiche que l'agent propose et rend des valeurs. **Aucune règle propre à un jeu** : libellés, bornes, choix, ressources et preuves sont ceux de la fiche (écart 14.26) ; une fiche de plus ne demande aucun code client. Le parcours (lecture, saisie, envoi, progression) est dans `composables/useFicheInstall.ts` et `composables/useDeployFlow.ts` (le même que celui d'un déploiement), la saisie en mémoire dans `stores/ficheInstall.ts`.

```
fiche/
├── proof.ts         → la preuve d'un chiffre de ressource : `proofOf`, `isMeasured`, `proofUrl`, `proven` (sans preuve, aucun nombre) — BR-FICHE-008, 057
├── answers.ts       → les questions jugées sur ce que la fiche dit (genre, bornes, choix, jeu de caractères, longueur), comme l'agent : `answerIssue` (des CODES, au plus UN caractère refusé), `visibleQuestions` (par variante), `answersToSend`, `changedAnswers` — BR-FICHE-016, 017, 038, 040, 060
├── ports.ts         → les ports : `publishedPorts`, `parseHostPort` (1024 à 65535), `readableHostPorts`, `duplicateHostPorts` — BR-FICHE-019
├── name.ts          → le nom du service proposé : `slugOf`, `proposedName` (-2, -3…) — BR-FICHE-015
├── fit.ts           → la machine et la place : `minimumVerdicts` (seul un minimum MESURÉ bloque), `coverage`, `diskVerdict`, `placement` (où placer les curseurs), `memoryNotices` — BR-FICHE-020 à 023
├── attention.ts     → les points d'attention de la fenêtre : `attentionPoints`, `allValidated`, `acceptOf` (seules les cases comprises par l'agent partent) — BR-FICHE-026, BR-RES-008
├── draft.ts         → la saisie (`FicheDraft`), `wipeFicheDraft`, `toInstallRequest` (la demande : fiche, version, empreinte lues, aucun champ libre) — BR-FICHE-013, 034
├── firstStart.ts    → le premier démarrage long : des faits, jamais un pourcentage — BR-FICHE-056
├── character.ts     → un caractère refusé dit en mots (`characterName` : « l'espace », « le point d'exclamation ») et le texte corrigé (`correctedText`, jamais pour un secret) — revue UX du 24/10, constat 5
├── image.ts         → le registre déduit du dépôt de l'image
├── messages.ts      → les codes de ces règles et les refus de l'agent mis en phrases (vouvoiement, impératif), jamais un texte de l'agent ; `valueLabel` (un choix par son libellé), `extraHint` (aide ajoutée seulement si la fiche ne la dit pas)
├── fiche.test.ts    → les règles, sur la fiche réelle de Terraria et sur une fiche d'essai qui porte tout
└── copy.test.ts     → règles de rédaction des textes `fiche.*` et `catalogue.*` : vouvoiement, pas de tiret cadratin, rien d'interne au moteur, jamais « isolé » ni « sûr » (BR-FICHE-036)
```

Les fiches simulées (`link/simulated-fiches.ts`) donnent la forme RÉELLE de `GET /catalog/terraria` et d'une fiche d'essai riche ; le pont simulé juge une installation comme l'agent (`link/simulated-catalog.ts`).
