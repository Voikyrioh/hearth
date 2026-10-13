# Mettre en service Calcy, la review automatique

Calcy relit le diff des branches que le propriétaire pousse et laisse UN commentaire sur la pull request ouverte (`.github/workflows/calcy-review.yml`, décision : [ADR-0056](../adr/ADR-0056-review-automatique-calcy-par-appel-direct-et-federation-d-identite.md)). Il n'agit sur rien : aucun outil, aucune approbation, aucune fusion. Tant que les étapes ci-dessous ne sont pas faites, le flux ne démarre pas (variable `CALCY_MODEL` absente) et ne coûte rien.

## Prérequis
- Droits d'administrateur du dépôt `Voikyrioh/hearth` et rôle administrateur ou propriétaire dans l'organisation Anthropic (console Claude Platform).
- `gh` connecté en tant que `Voikyrioh`.
- Déjà en place depuis le 09/10 (identifiants non secrets, écrits dans le flux) : émetteur GitHub Actions, règle de fédération `fdrl_01AsnGjCeaCTjwCHw8RyQzGK`, compte de service `hearth-ci-review` (`svac_01NuXPZUAvyw1uzrcpMVcB5F`), espace de travail `hearth-ci` (`wrkspc_017gUxiWv2q7JPVtVhXgfP8o`). À contrôler dans la console, sous Settings, Workload identity.

## Étapes, dans l'ordre

1. **Fusionner la PR qui introduit Calcy** (le script doit être sur `main` : le flux le prend sur `main`, jamais sur la branche relue). Tant qu'il n'y est pas, le flux affiche une note et sort sans rien faire.

2. **Poser un plafond de dépense sur l'espace de travail `hearth-ci`** dans la console Anthropic (c'est le SEUL garde-fou dur du coût ; le script n'en a pas). Valeur conseillée : 10 USD par mois pour commencer. Emplacement exact dans la console : non vérifié ; à défaut de plafond par espace, regarder les limites de dépense de l'organisation.

3. **Resserrer la règle de fédération** (recommandé, car sans cela toute personne ayant le droit d'écrire dans le dépôt pourrait pousser une branche au flux modifié) : console, Settings, Workload identity, règle `fdrl_01AsnGjCeaCTjwCHw8RyQzGK`, ajouter la correspondance de revendication `actor` = `Voikyrioh` en plus des conditions actuelles (`event_name=push`, `repository`, `repository_owner`, `repository_owner_id`). La revendication `actor` figure dans le jeton GitHub décrit par la documentation ; son acceptation comme correspondance de la règle n'est pas vérifiée ici. Le panneau « authentication history » de la même page montre la raison d'un refus.

4. **Vérifier le rôle du compte de service** : la règle donne `workspace:developer` (même accès qu'une clé d'espace de travail). Le restreindre s'il existe un rôle plus étroit qui permet d'appeler les messages ; non vérifié.

5. **Poser les variables du dépôt** (aucun secret n'est nécessaire) :
   ```sh
   gh variable set CALCY_MODEL --repo Voikyrioh/hearth --body claude-opus-5-5
   gh variable set CALCY_PRICE_INPUT_PER_MTOK --repo Voikyrioh/hearth --body 4
   gh variable set CALCY_PRICE_OUTPUT_PER_MTOK --repo Voikyrioh/hearth --body 20
   ```
   `claude-opus-5-5` est l'identifiant d'API d'Opus 5.5 donné par la documentation (modèle de review retenu avec Voiky le 09/10). Les prix (4 USD en entrée, 20 USD en sortie par million de jetons) sont ceux de la page de tarifs au 13/10/2026 : https://platform.claude.com/docs/en/about-claude/pricing ; à remettre à jour si elle change, ils servent seulement à la ligne « coût estimé » du commentaire. Facultatif : `gh variable set CALCY_MAX_DIFF_CHARS --repo Voikyrioh/hearth --body 200000` (valeur par défaut).

6. **Vérifier qu'aucune clé d'API ne traîne** : `gh secret list --repo Voikyrioh/hearth` ne doit pas montrer `ANTHROPIC_API_KEY`. Une telle clé passerait avant la fédération et serait utilisée à sa place.

## Vérifier sur une première PR
1. Créer une petite branche avec un changement visible, la pousser, ouvrir la PR (pas en brouillon).
2. Suivre l'exécution : `gh run list --repo Voikyrioh/hearth --workflow calcy-review.yml --limit 3` puis `gh run view <id> --repo Voikyrioh/hearth --log`. Attendu : une ligne `calcy: PR #n against origin/main`, une estimation de jetons, `calcy: review received`, `calcy: posted a new comment`. Aucun jeton, aucun nom de fichier dans les journaux.
3. Lire le commentaire : `gh pr view <n> --repo Voikyrioh/hearth --comments`. Attendu : verdict en première ligne, constats localisés, section « Lu et non vérifié », et un pied avec le sha, le modèle, les jetons et le coût estimé. Pas de phrase d'accroche.
4. Pousser un second commit : le MÊME commentaire est mis à jour (pas un deuxième).
5. Essais de garde : PR en brouillon (le job sort avec « skipped, the pull request is a draft »), commit contenant `[skip calcy]`, branche sans PR.
6. Essai sans réseau à tout moment : `node .github/scripts/calcy-review.mjs --dry-run --diff .github/calcy/exemples/exemple.diff --review .github/calcy/exemples/exemple-review-hostile.md`.

## Couper Calcy
- Une commande : `gh workflow disable calcy-review.yml --repo Voikyrioh/hearth` (réactiver : `gh workflow enable calcy-review.yml --repo Voikyrioh/hearth`).
- Alternative : `gh variable delete CALCY_MODEL --repo Voikyrioh/hearth` (le job ne démarre plus).
- Arrêt côté Anthropic : désactiver la règle de fédération ou le compte de service dans la console (aucun appel ne passe plus, même si le flux tourne).
- Pour une PR ou un commit : la passer en brouillon, ou `[skip calcy]` dans le message du dernier commit poussé.

## Combien ça coûte par poussée
Prix lus dans la documentation (13/10/2026) pour Opus 5.5 : 4 USD par million de jetons en entrée, 20 USD en sortie. Estimation de l'entrée : (caractères du diff + environ 8 500 caractères de consignes et d'encadrement) / 3 caractères par jeton (3 est une hypothèse prudente pour du code, NON vérifiée pour le découpage d'Opus 5.5).

| Cas | Entrée | Sortie | Coût |
|---|---|---|---|
| Petit diff, 5 000 caractères | environ 4 500 jetons, soit 0,018 USD | 800 jetons, soit 0,016 USD | environ 0,03 USD |
| Diff courant, 30 000 caractères | environ 12 800 jetons, soit 0,051 USD | 1 500 jetons, soit 0,030 USD | environ 0,08 USD |
| Plafond, 200 000 caractères | environ 69 500 jetons, soit 0,278 USD | 4 000 jetons (maximum), soit 0,080 USD | environ 0,36 USD |

Le coût se compte par POUSSÉE, pas par PR : dix poussées sur un diff courant font environ 0,80 USD. Au-delà de 200 000 caractères relisables, aucun appel n'est fait. Si Sonnet 5.5 est choisi (`CALCY_MODEL=claude-sonnet-5-5`, 2 USD / 10 USD) le coût est environ divisé par deux. Que ces appels soient imputés sur les crédits d'API mensuels du plan Max (support.claude.com, article 17154008) ou facturés en plus : non vérifié ; regarder la console après la première semaine.

## Dépannage
- **Le job ne démarre pas** : `CALCY_MODEL` non posée, acteur différent du propriétaire, branche `main` ou `dependabot/**`, ou flux désactivé.
- **`Anthropic token exchange: HTTP 401`** : la règle refuse le jeton (message volontairement opaque). Console, Workload identity, historique d'authentification : la raison y figure (`match_subject_prefix`, condition d'acteur, audience…). Vérifier aussi que la condition `actor` ajoutée à l'étape 3 est exacte.
- **`HTTP 404` ou `invalid_request_error` sur le modèle** : identifiant de `CALCY_MODEL` faux ; relire la page des modèles de la documentation.
- **`HTTP 429` ou plafond de dépense atteint** : le job échoue sans nouvelle tentative (voulu). Relever le plafond ou attendre.
- **Commentaire « Diff trop gros »** : découper la PR, ou relever `CALCY_MAX_DIFF_CHARS` en connaissance du coût.
- **Rien dans les journaux sur `calcy-review.mjs`** : le script n'est pas encore sur `main` (étape 1).

## Repli si la fédération devenait indisponible
Non mis en service, à décider avec le propriétaire : créer un environnement GitHub `calcy` (relecteur obligatoire : le propriétaire, branches autorisées limitées), y poser une clé d'API dédiée à l'espace de travail plafonné, et restreindre le flux aux PR du dépôt lui-même. La fédération reste préférable : rien à garder ni à faire tourner.
