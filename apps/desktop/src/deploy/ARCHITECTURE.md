# Architecture — apps/desktop/src/deploy

Déployer une image depuis le client (HRT-69, ADR-0039, `BR-DEPLOY-*`). Règles PURES, sans E/S, sans Vue ni Pinia : ce dossier ne parle ni au pont (`link/`) ni à un magasin ; il ne reçoit que des valeurs et rend des valeurs. Le parcours (consultation en direct, envoi, progression) est dans `composables/useDeployForm.ts` et `composables/useDeployFlow.ts`, la saisie en mémoire dans `stores/deployForm.ts`.

```
deploy/
├── rules.ts        → le miroir de ce que l'agent juge (`crates/hearth-agent/src/domain/services/{deploy,command}.rs`) : `nameIssue` (2 à 63), `splitImage`, `imageIssue`, `tagIssue`, `envNameIssue`, `envValueIssue`, `looksSecret`, `parsePort`, `splitCommand` / `commandIssue` (sans interpréteur, `isWhitespace` comme `char::is_whitespace`). Des CODES, jamais des textes ni la valeur reçue. Les règles de CONFLIT (qui tient quoi) ne sont pas ici : elles sont jugées par l'agent — BR-DEPLOY-003, 005, 006, 007, 014
├── draft.ts        → le brouillon (`Draft`, `EnvRow`, `PortRow`), `isFilled`, `wipeDraft` (écrase les valeurs puis lâche tout), `movedVersion` — BR-DEPLOY-005, 029
├── form.ts         → ce qu'on tire d'un brouillon : `formIssues` (défauts de forme par rangée), `toCheckInput` (la consultation, SANS secret), `toRequest` (la demande, ressources obligatoires), `shownVersion` — BR-DEPLOY-033, BR-DEPLOY-038
├── messages.ts     → les codes de l'agent mis en phrases (jamais un texte de l'agent) : `portVerdict`, `checkBlocker` (UNE règle « ce constat bloque »), `refusalText`, `failureText`, `deployOutcome` (notifications) — BR-DEPLOY-008, 017, 019, 032
├── availability.ts → la « porte » du bouton `deployDoor`, `routeOpen` (garde de la route), `submitReason` (pourquoi « Déployer » est inactif) — BR-DEPLOY-001, 027
├── deploy.test.ts  → les vecteurs de l'agent, la porte, les phrases
└── copy.test.ts    → règles de rédaction des textes `deploy.*` : vouvoiement, pas de tiret cadratin, rien d'interne au moteur, jamais « isolé » ni « sûr » — BR-DEPLOY-036
```

Une règle nouvelle côté agent se recopie ici avec son vecteur de test ; une borne partagée n'a pas encore de fiche commune (suivi).
