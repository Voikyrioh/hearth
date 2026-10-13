# Mettre en service Calcy, la review automatique

> **À FAIRE TOUT DE SUITE, avant toute autre étape : tant que la règle de fédération actuelle (large) existe, désactive-la dans la console Anthropic.** Elle accepte n'importe quel flux, de n'importe quelle branche, de n'importe quel acteur, pourvu que ce soit une poussée dans ce dépôt, et elle est active depuis le 09/10 indépendamment de cette PR. Console, Settings, Workload identity, règle `fdrl_…` (celle de l'espace `hearth-ci`), la désactiver. Tu la réactiveras à l'étape 5, resserrée.

Calcy relit le diff des branches que le propriétaire pousse et laisse UN commentaire sur la pull request ouverte (décision : [ADR-0056](../adr/ADR-0056-review-automatique-calcy-par-appel-direct-et-federation-d-identite.md)). Il n'agit sur rien : aucun outil, aucune approbation, aucune fusion. Tant que la variable `CALCY_MODEL` n'est pas posée (étape 4), le flux ne démarre pas et ne coûte rien.

## Comment c'est construit, et ce qui protège vraiment

- `.github/workflows/calcy-review.yml` : petit flux déclencheur sur `push`. Il ne fait rien de sensible et **ne protège de rien** : sur un `push`, GitHub exécute le fichier tel qu'il est dans le commit poussé, donc quiconque peut pousser une branche réécrit ce fichier, filtre d'acteur compris. Ses `if:` ne servent qu'à ne pas lancer de machine pour rien (économie de minutes de CI).
- `.github/workflows/calcy-reusable.yml` : flux réutilisable (`workflow_call`) qui contient tout le travail sensible : jeton d'identité, comptage de jetons, appel à l'API, commentaire. Le déclencheur l'appelle PAR RÉFÉRENCE À `main` (`uses: Voikyrioh/hearth/.github/workflows/calcy-reusable.yml@main`). Le jeton d'identité GitHub porte alors la revendication `job_workflow_ref` = `Voikyrioh/hearth/.github/workflows/calcy-reusable.yml@refs/heads/main`, quelle que soit la branche poussée.
- **La barrière est la règle de fédération côté Anthropic** : elle n'accepte que ce `job_workflow_ref`. Une branche qui réécrit le déclencheur pour faire autre chose (appeler le fichier de sa propre branche, ou un autre fichier) obtient quand même un jeton chez GitHub, mais Anthropic le refuse. Ce qu'un pousseur peut encore faire : déclencher la review normale, sur son propre diff, avec le script de `main`.
- Preuves dans la documentation : `job_workflow_ref` est une revendication du jeton OIDC de GitHub, « the ref path to the reusable workflow », exemple `octo-org/octo-automation/.github/workflows/oidc.yml@refs/heads/main` ; `id-token: write` doit être accordé au niveau du flux ou du job APPELANT, un flux appelé ne peut que garder ou réduire les permissions de l'appelant ([OIDC GitHub](https://docs.github.com/en/actions/reference/security/oidc), [flux réutilisables](https://docs.github.com/en/actions/reference/workflows-and-actions/reusing-workflow-configurations)). Une règle Anthropic accepte `subject_prefix`, `audience`, `claims` (« each key is a top-level claim name and each value is the required exact string value ») et une expression CEL `condition` sur l'ensemble des revendications du jeton ([référence WIF](https://platform.claude.com/docs/en/manage-claude/wif-reference#rule-matching-semantics)). Donc `job_workflow_ref` et `actor_id`, revendications de premier niveau en chaînes, sont exprimables.
- **Non vérifié** : que la console (et non seulement l'API d'administration) accepte ces noms de revendications dans le bloc des revendications. S'il les refuse : essaie la même règle en expression CEL (`claims.job_workflow_ref == "…" && …`) ; s'il les refuse aussi, repli sur un environnement protégé (section « Repli », plus bas).

## Prérequis
- Droits d'administrateur du dépôt `Voikyrioh/hearth` et rôle administrateur ou propriétaire dans l'organisation Anthropic (console Claude Platform).
- `gh` connecté en tant que `Voikyrioh`.
- Déjà en place depuis le 09/10 : émetteur GitHub Actions, règle de fédération, compte de service `hearth-ci-review`, espace de travail `hearth-ci`. Leurs identifiants (`fdrl_…`, `svac_…`, `wrkspc_…`, identifiant d'organisation) se lisent dans la console, Settings, Workload identity ; ils ne sont écrits dans aucun fichier du dépôt (ceux de la première version de cette branche restent dans son historique : la branche est fusionnée en un seul commit écrasé).

## Étapes, dans l'ordre

0. **Désactiver la règle actuelle** dans la console (voir l'encadré en tête).

1. **Fusionner la PR qui introduit Calcy, AVANT tout le reste.** Le déclencheur appelle `calcy-reusable.yml@main` : tant que le fichier n'est pas sur `main`, chaque poussée d'une branche (celle de la PR comprise) produit une exécution en échec « startup failure » (flux appelé introuvable). C'est attendu et sans effet. Fusion en un seul commit écrasé.

2. **Vérifier la protection de `main`** : pas de poussée directe, PR obligatoire, relecture du propriétaire du code obligatoire (`.github/CODEOWNERS` désigne le propriétaire pour `/.github/`). Sans cela, quiconque modifie `main` modifie le fichier que la règle fait confiance. Lecture : `gh api repos/Voikyrioh/hearth/branches/main/protection --jq '{pr: .required_pull_request_reviews, owners: .required_pull_request_reviews.require_code_owner_reviews, admins: .enforce_admins.enabled}'`. Vérifier aussi, dans Settings, Collaborators and teams, qu'aucune personne ni équipe n'a le droit d'écriture, et dans Settings, GitHub Apps, qu'aucune application installée n'a les droits `contents` et `workflows` en écriture. Jamais de clé de déploiement en écriture.

3. **Poser un plafond de dépense sur l'espace de travail `hearth-ci`** dans la console Anthropic. C'est le SEUL plafond qui coupe vraiment la dépense quoi qu'il arrive (le script a ses limites, mais elles sont dans le script). Valeur conseillée : 10 USD par mois pour commencer. Emplacement exact dans la console : non vérifié ; à défaut de plafond par espace, regarder les limites de dépense de l'organisation. Le constater, pas seulement le décider.

4. **Poser les variables du dépôt** (aucun secret). Les quatre identifiants de la fédération se copient depuis la console (Settings, Workload identity ; l'identifiant d'organisation sous Settings, Organization) :
   ```sh
   gh variable set CALCY_FEDERATION_RULE_ID    --repo Voikyrioh/hearth --body '<fdrl_… de la règle>'
   gh variable set CALCY_ORGANIZATION_ID       --repo Voikyrioh/hearth --body '<identifiant d organisation>'
   gh variable set CALCY_SERVICE_ACCOUNT_ID    --repo Voikyrioh/hearth --body '<svac_… de hearth-ci-review>'
   gh variable set CALCY_WORKSPACE_ID          --repo Voikyrioh/hearth --body '<wrkspc_… de hearth-ci>'
   gh variable set CALCY_PRICE_INPUT_PER_MTOK  --repo Voikyrioh/hearth --body 4
   gh variable set CALCY_PRICE_OUTPUT_PER_MTOK --repo Voikyrioh/hearth --body 20
   # EN DERNIER : c'est l'interrupteur (le flux ne démarre pas sans lui)
   gh variable set CALCY_MODEL                 --repo Voikyrioh/hearth --body claude-opus-5-5
   ```
   Ce sont des variables, pas des secrets : un flux affiche la valeur de ses variables dans l'en-tête de l'étape, donc dans le journal d'exécution, public pour un dépôt public. Elles ne sont plus dans les fichiers du dépôt, mais ne les crois pas cachées : ce qui protège, c'est la règle étroite de l'étape 5. `claude-opus-5-5` est l'identifiant d'API d'Opus 5.5 donné par la documentation (modèle de review retenu avec Voiky le 09/10). Les prix (4 et 20 USD par million de jetons) sont ceux de la page de tarifs au 13/10/2026 : https://platform.claude.com/docs/en/about-claude/pricing ; ils servent seulement à la ligne de coût du commentaire. Facultatifs : `CALCY_MAX_DIFF_CHARS` (200000 par défaut) et `CALCY_MAX_INPUT_TOKENS` (60000 par défaut, voir « Combien ça coûte »).

5. **Poser la règle de fédération étroite** (console, Settings, Workload identity, règle `fdrl_…`, modification, puis la réactiver). Valeurs exactes :

   | Champ | Valeur |
   |---|---|
   | Préfixe de sujet (`subject_prefix`) | `repo:Voikyrioh@22913150/hearth@1404653782:ref:refs/heads/*` (le préfixe actuel de la règle, qui porte les identifiants numériques, avec `:*` remplacé par `:ref:refs/heads/*` : plus d'étiquette, plus de pull request, plus d'environnement) |
   | Audience | `https://api.anthropic.com` |
   | Revendication `event_name` | `push` |
   | Revendication `repository` | `Voikyrioh/hearth` |
   | Revendication `repository_owner` | `Voikyrioh` |
   | Revendication `repository_owner_id` | `22913150` |
   | Revendication `actor_id` | `22913150` (l'identifiant numérique de ton compte, pas son nom : un nom peut être repris) |
   | Revendication `job_workflow_ref` | `Voikyrioh/hearth/.github/workflows/calcy-reusable.yml@refs/heads/main` |
   | Portée (`oauth_scope`) | `workspace:inference` si la console le propose (Messages, comptage de jetons, modèles : tout ce que Calcy utilise ; la page des portées le décrit), sinon `workspace:developer` |
   | Durée du jeton (`token_lifetime_seconds`) | 300 (minimum 60 ; le jeton GitHub vit environ 5 minutes et l'échange est fait au début du job) |

   Même règle en une expression CEL, si les revendications séparées sont refusées : `claims.event_name == "push" && claims.repository == "Voikyrioh/hearth" && claims.repository_owner_id == "22913150" && claims.actor_id == "22913150" && claims.job_workflow_ref == "Voikyrioh/hearth/.github/workflows/calcy-reusable.yml@refs/heads/main"` (avec le préfixe de sujet à côté). Le panneau « authentication history » de la même page montre, pour chaque essai, les revendications lues et la raison d'un refus.

6. **Essai NÉGATIF (obligatoire, à faire par toi)** : une branche qui réécrit le déclencheur ne doit PAS obtenir de jeton.
   1. `git switch -c essai-calcy-negatif main`
   2. Dans `.github/workflows/calcy-review.yml`, remplace la ligne `uses: Voikyrioh/hearth/.github/workflows/calcy-reusable.yml@main` par `uses: ./.github/workflows/calcy-reusable.yml` (référence locale : suit la branche). Commit, `git push -u origin essai-calcy-negatif`.
   3. Ouvre la pull request (pas en brouillon) : `gh pr create --repo Voikyrioh/hearth --base main --head essai-calcy-negatif --title "essai negatif Calcy" --body "essai, a fermer"`. Le script ne tente l'échange que si une PR ouverte existe au moment de la poussée : pousse donc un commit vide APRÈS : `git commit --allow-empty -m "essai" && git push`.
   4. `gh run list --repo Voikyrioh/hearth --workflow calcy-review.yml --limit 3`, puis `gh run view <id> --repo Voikyrioh/hearth --log`. **Attendu** : le job échoue à l'étape de review avec `calcy: Anthropic token exchange: HTTP 401`, aucun commentaire n'apparaît sur la PR, et la console, historique d'authentification, montre un échange REFUSÉ pour cet essai. **Si un commentaire de Calcy apparaît, ou si l'échange réussit : la règle est trop large. Désactive-la immédiatement et ne passe pas à l'étape 7.**
   5. Nettoyage : `gh pr close <n> --repo Voikyrioh/hearth --delete-branch`.

7. **Essai POSITIF** : voir « Vérifier sur une première PR » ci-dessous. S'il échoue en 401 alors que l'essai négatif est correct, ouvre l'historique d'authentification, compare la valeur reçue de chaque revendication à la règle (la forme exacte de `job_workflow_ref`, `actor_id` en chaîne) et corrige la règle, pas le flux.

8. **Vérifier qu'aucune clé d'API ne traîne** : `gh secret list --repo Voikyrioh/hearth` ne doit pas montrer `ANTHROPIC_API_KEY`. Une telle clé passerait avant la fédération et serait utilisée à sa place.

## Vérifier sur une première PR
1. Créer une petite branche avec un changement visible, la pousser, ouvrir la PR (pas en brouillon).
2. Suivre l'exécution : `gh run list --repo Voikyrioh/hearth --workflow calcy-review.yml --limit 3` puis `gh run view <id> --repo Voikyrioh/hearth --log`. Attendu : `calcy: PR #n against origin/main`, `calcy: input tokens counted by the API: …`, `calcy: review received`, `calcy: posted a new comment`. Aucun jeton, aucun nom de fichier dans les journaux.
3. Lire le commentaire : `gh pr view <n> --repo Voikyrioh/hearth --comments`. Attendu : en tête la ligne fixe « Relecture automatique par un modèle (Calcy)… ne vaut pas approbation », puis la réponse du modèle dans un bloc de code (texte brut, verdict en première ligne), puis « Non lu, ou à lire avec prudence » avec les NOMS des fichiers soustraits, puis un pied avec le sha, le modèle et les jetons FACTURÉS renvoyés par l'API. Pas de phrase d'accroche.
4. À l'essai, regarde en particulier : que la réponse ne soit pas coupée ou vide (si le modèle raisonne par défaut, ce raisonnement se déduit des 4000 jetons de sortie : le pied dit alors « limite de jetons de sortie » ; non vérifié) ; que `git diff --attr-source` marche sur le git du runner (il faut git 2.40 ou plus ; sinon le job échoue avec `git diff failed`, sans rien dépenser) ; que les variables `CALCY_*` arrivent bien dans le flux appelé (sinon `missing environment variable`).
5. Pousser un second commit : le MÊME commentaire est mis à jour (pas un deuxième).
6. Essais de garde : PR en brouillon (« skipped, the pull request is a draft »), commit contenant `[skip calcy]`, branche sans PR. Une PR qui sort du brouillon n'est relue qu'à la poussée suivante.
7. Essais sans réseau à tout moment : `node .github/scripts/calcy-review.mjs --dry-run --diff .github/calcy/exemples/exemple.diff --review .github/calcy/exemples/exemple-review-hostile.md` ; tests : `node --test .github/scripts/calcy-review.test.mjs` (Node 20 ou plus, sans dépendance).

## Couper Calcy
- Une commande : `gh workflow disable calcy-review.yml --repo Voikyrioh/hearth` (réactiver : `gh workflow enable calcy-review.yml --repo Voikyrioh/hearth`).
- Alternative : `gh variable delete CALCY_MODEL --repo Voikyrioh/hearth` (le job ne démarre plus).
- Arrêt côté Anthropic, le plus sûr : désactiver la règle de fédération ou le compte de service dans la console (aucun appel ne passe plus, même si un flux tourne).
- Pour une PR ou un commit : la passer en brouillon, ou `[skip calcy]` dans le message du dernier commit poussé.

## Combien ça coûte par poussée
Prix lus dans la documentation (13/10/2026) pour Opus 5.5 : 4 USD par million de jetons en entrée, 20 USD en sortie (Sonnet 5.5 : 2 et 10).

Le plafond est exprimé en JETONS, pas en caractères. Avant chaque appel payant, le script demande à l'API de compter les jetons du message exact (point d'accès de comptage, gratuit : https://platform.claude.com/docs/en/build-with-claude/token-counting) ; au-dessus de `CALCY_MAX_INPUT_TOKENS` (60000 par défaut), il poste un avis et ne fait aucun appel payant. La documentation précise que ce comptage est une ESTIMATION (« might differ by a small amount ») : le plafond est donc pratique, pas une preuve au jeton près. La sortie est fixée à 4000 jetons au maximum. Une limite de 200000 caractères sur le diff écarte avant tout appel les diffs énormes. Si le comptage échoue, le job échoue : jamais d'appel payant « à l'aveugle ».

| | Entrée | Sortie | Coût maximal par poussée |
|---|---|---|---|
| Opus 5.5 (4 / 20 USD) | 60000 jetons, soit 0,240 USD | 4000 jetons, soit 0,080 USD | environ 0,32 USD |
| Sonnet 5.5 (2 / 10 USD) | 60000 jetons, soit 0,120 USD | 4000 jetons, soit 0,040 USD | environ 0,16 USD |

Un diff courant (quelques milliers de lignes de code) tient en quelques dizaines de milliers de jetons au plus ; le pied de chaque commentaire donne les jetons réellement facturés. Le coût se compte par POUSSÉE, pas par PR. Les consignes, le titre et la description sont bornés par des constantes dans le script (environ 7 200, 300 et 10 000 caractères) et comptés dans le total. Que ces appels soient imputés sur les crédits d'API mensuels du plan Max (support.claude.com, article 17154008) ou facturés en plus : non vérifié ; regarder la console après la première semaine. Seul le plafond de l'espace de travail (étape 3) coupe tout, y compris si le script était contourné.

## Dépannage
- **Le job ne démarre pas** : `CALCY_MODEL` non posée, acteur différent du propriétaire, branche `main` ou `dependabot/**`, ou flux désactivé. **Exécution « startup failure »** : le flux appelé n'est pas encore sur `main` (étape 1).
- **`Anthropic token exchange: HTTP 401`** : la règle refuse le jeton (message volontairement opaque). Console, Workload identity, historique d'authentification : la raison y figure. Causes probables : un identifiant de variable faux, une revendication de la règle qui ne correspond pas (le flux n'est pas appelé par référence à `main`, ou l'acteur n'est pas toi), ou la règle encore désactivée.
- **`missing environment variable ANTHROPIC_…`** : une des variables `CALCY_FEDERATION_RULE_ID`, `CALCY_ORGANIZATION_ID`, `CALCY_SERVICE_ACCOUNT_ID`, `CALCY_WORKSPACE_ID` n'est pas posée (ou n'est pas lue par le flux appelé).
- **`HTTP 404` ou `invalid_request_error` sur le modèle** : identifiant de `CALCY_MODEL` faux ; relire la page des modèles de la documentation.
- **`HTTP 429` ou plafond de dépense atteint** : le job échoue sans nouvelle tentative (voulu). Relever le plafond ou attendre.
- **Commentaire « Diff trop gros » ou « Message trop gros »** : découper la PR, ou relever `CALCY_MAX_DIFF_CHARS` / `CALCY_MAX_INPUT_TOKENS` en connaissance du coût.
- **`git diff failed`** : le git du runner est plus ancien que 2.40 (l'option `--attr-source` manque). Le job échoue plutôt que de lire un diff que la branche pourrait avoir masqué.

## Repli si la règle ne peut pas exprimer `job_workflow_ref` : environnement protégé
Plus solide encore (un clic humain par exécution, ce qui borne aussi le coût), mais plus automatique. À n'utiliser que si ni les revendications séparées ni la condition CEL ne sont acceptées par la console.
1. Créer l'environnement `calcy` (Settings, Environments) avec **relecteur obligatoire** (toi) et, dans « Deployment branches », seulement les branches du dépôt. Un environnement SANS relecteur obligatoire ne vaut rien : n'importe quelle branche peut le déclarer.
2. Dans `calcy-reusable.yml` (donc sur `main`, par PR), ajouter `environment: calcy` au job `review`.
3. Règle de fédération : sujet EXACT `repo:Voikyrioh@22913150/hearth@1404653782:environment:calcy` (forme documentée des deux côtés : `repo:<owner>/<repo>:environment:<name>`, avec les identifiants numériques comme dans le sujet actuel de la règle), plus `event_name=push`, `repository_owner_id`, `actor_id`. Essai négatif identique à l'étape 6 : la branche qui déclare l'environnement attend l'approbation, et celle qui ne le déclare pas n'a pas le bon sujet.
