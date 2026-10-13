---
id: ADR-0056
titre: Review automatique « Calcy » des branches du propriétaire : appel direct à l'API Messages, fédération d'identité GitHub, déclenchement sur push, script pris sur main, sortie du modèle inerte
type: securite
statut: proposée (non validée par Voiky ; pas encore en service, voir le runbook)
date: 2026-10-13
portee: repo
remplace: aucune
liens: [ADR-0008, runbook mettre-en-service-calcy, SECURITY.md]
---

# ADR-0056 : Review automatique « Calcy »

> Brouillon du 09/10, repris et durci le 13/10. Il portait le numéro 0035, déjà pris par la décision sur les mises à jour servies par `hearth.voikyrioh.fr`.

## Contexte
Le propriétaire veut un premier regard automatique sur ses propres pull requests : un commentaire qui relit le diff (sécurité, tests, cohérence de la doc, texte de l'interface), dans la voix d'un relecteur nommé Calcy (professionnel, de petits emoji de flamme, des jeux de mots sur le feu seulement quand le contexte les rend évidents, aucun nom ni réplique tirés d'une œuvre existante). Le dépôt est PUBLIC : le flux ne doit contenir aucun secret, ne doit jamais s'exécuter pour le compte d'une autre personne, et ne doit pas pouvoir faire dépenser de l'argent sans l'accord du propriétaire.

## Décision
1. **Appel direct à l'API Messages** (`POST /v1/messages`) depuis un script Node sans dépendance (`.github/scripts/calcy-review.mjs`), et non l'action officielle de Claude Code, dont l'usage n'est pas couvert par les crédits d'API mensuels du propriétaire (https://support.claude.com/en/articles/17154008-monthly-api-credits-for-max-and-team-plans). Aucune action tierce hors `actions/checkout`, épinglée par empreinte comme `ci.yml`.
2. **Fédération d'identité, aucune clé d'API.** Le mode existe et fonctionne comme le flux le suppose : Workload Identity Federation, disponible en général depuis juin 2026 (https://platform.claude.com/docs/en/manage-claude/workload-identity-federation, https://platform.claude.com/docs/en/manage-claude/wif-providers/github-actions). Le flux demande à GitHub un jeton OIDC d'audience `https://api.anthropic.com`, l'échange (`POST /v1/oauth/token`, grant `urn:ietf:params:oauth:grant-type:jwt-bearer`, plus règle, organisation, compte de service et espace de travail) contre un jeton d'accès `sk-ant-oat01-…` de courte durée, puis appelle l'API. Les identifiants `fdrl_`, `svac_`, `wrkspc_` et l'organisation ne sont pas secrets et sont écrits en clair dans le flux ; les jetons sont masqués (`::add-mask::`) et jamais affichés. Un `ANTHROPIC_API_KEY` ne doit exister nulle part (il passerait avant la fédération dans l'ordre de résolution des identifiants de la documentation).
3. **Déclenchement sur `push`, jamais `pull_request` ni `pull_request_target`.** La règle côté Anthropic n'accepte que `event_name=push` de ce dépôt. Un `push` n'existe que pour qui a le droit d'écrire dans le dépôt : une PR venue d'un fork ne démarre pas ce flux, n'obtient aucun jeton OIDC utile et ne dépense rien. Le flux ignore `main` et `dependabot/**` et exige que l'acteur soit le propriétaire. Il retrouve la PR ouverte de la branche par l'API GitHub et ne continue que si la PR est celle du propriétaire, vient de ce dépôt (pas d'un fork), n'est pas en brouillon, correspond encore au commit poussé, et que le dernier message de commit ne contient pas `[skip calcy]`.
4. **Le code exécuté vient de `main`, jamais de la branche relue.** Deux extractions : `trusted/` (branche `main`, uniquement `.github/scripts` et `.github/calcy`) pour le script et les consignes ; `untrusted/` (le commit poussé, historique complet) lu comme donnée par `git diff --no-ext-diff --no-textconv`. Rien de `untrusted/` n'est exécuté (ni script, ni build, ni test, ni hook : pas d'identifiants de dépôt conservés). Tant que le script n'est pas sur `main`, le flux le constate et sort sans rien faire.
5. **Le contenu de la PR est une donnée hostile.** Titre, description, noms de fichiers, messages de commit et diff ne sont jamais placés dans une ligne `run:` ni dans une expression `${{ }}` : seuls le nom de branche, le sha, le dépôt et des variables du dépôt passent, par `env:`. Dans le message envoyé au modèle, ils sont encadrés par des balises à marqueur aléatoire (impossible à refermer depuis le diff) et les consignes (`system`) interdisent de leur obéir. Les noms de fichiers sont nettoyés (caractères de contrôle, retours à la ligne, accents graves) et ne sont jamais écrits dans les journaux, pour qu'aucune commande de flux (`::…`) ne puisse en sortir.
6. **Le modèle ne peut rien faire d'autre qu'écrire un texte.** Aucun outil n'est déclaré dans la requête, aucune action n'est lue dans la réponse. Le texte est nettoyé avant d'être posté : plus de HTML (`<` devient `&lt;`), plus d'image, liens affichés en clair entre accents graves (rien de cliquable), URL nues entre accents graves, `@` suivi d'un caractère invisible (aucune mention, aucune notification), références `dépôt/dépôt#n` neutralisées, `/` en début de ligne échappé (aucune commande lisible par un autre robot), blocs de code laissés tels quels mais refermés s'ils sont ouverts, taille bornée (20 000 caractères de review, 60 000 de commentaire), marqueur du commentaire écrit par le script seulement. Le script ne retrouve pour la mise à jour que le commentaire du compte `github-actions[bot]` portant le marqueur.
7. **Coût borné.**
   - Modèle lu dans la variable de dépôt `CALCY_MODEL` (jamais écrit dans le flux ; format `[A-Za-z0-9._-]` contrôlé) ; variable absente : le flux ne démarre pas. Le choix du modèle est un réglage, pas un changement de code.
   - Diff relisable (hors verrous, images, binaires) limité à 200 000 caractères (variable facultative `CALCY_MAX_DIFF_CHARS`). Au-delà : un commentaire le dit, AUCUN appel n'est fait.
   - Un seul appel par poussée, sans nouvelle tentative (une nouvelle tentative pourrait payer deux fois) ; `max_tokens` 4 000 ; un nouveau push de la même branche annule l'exécution précédente ; délai du job 10 minutes ; aucun cache.
   - Pied du commentaire : sha, modèle, jetons en entrée et en sortie, et le coût estimé SI les variables `CALCY_PRICE_INPUT_PER_MTOK` et `CALCY_PRICE_OUTPUT_PER_MTOK` (USD par million de jetons) sont posées, pour ne jamais figer un prix dans le dépôt.
   - Garde contre les boucles : un commentaire ne déclenche rien (le flux ne réagit qu'à `push`) ; les branches de robots (Dependabot) et tout acteur autre que le propriétaire sont ignorés.
8. **Permissions du jeton GitHub** : aucune par défaut (`permissions: {}`), puis sur le seul job `contents: read`, `pull-requests: write` (le commentaire) et `id-token: write` (nécessaire à la fédération : sans lui, GitHub ne délivre pas de jeton OIDC).
9. **Échec** : si l'échange ou l'appel échoue, le job échoue avec le code HTTP, le type d'erreur et le `request_id` (jamais un jeton), sans commenter la PR.
10. **Commentaire seul.** Aucune approbation, aucune fusion, aucune modification de code.

## Menaces et parades
| Menace | Parade |
|---|---|
| PR d'un fork qui fait dépenser de l'argent ou lit l'identité | Déclencheur `push` seulement ; la règle de fédération exige `event_name=push` ; condition d'acteur et de dépôt dans le flux. |
| Code de la PR exécuté avec accès à l'identité | Script pris sur `main` ; la branche relue n'est que lue par `git diff` (sans pilote de diff externe) ; pas de `pull_request_target`. |
| Injection d'instructions dans le diff, le titre, la description | Données encadrées par un marqueur aléatoire, consignes `system` qui l'interdisent, aucun outil donné au modèle, sortie traitée comme du texte inerte. Reste possible : un verdict faussé (voir Limites). |
| Injection de commande de flux ou de shell par un nom de fichier ou de branche | Aucune valeur d'auteur dans `run:` ni dans une expression ; noms nettoyés et absents des journaux ; ligne de journal toujours sur une seule ligne. |
| Fuite par la sortie du modèle (image distante, lien, mention, commande `/…`, HTML) | Nettoyage du texte avant publication (point 6). |
| Dépense incontrôlée (gros diff, boucle, poussées en rafale) | Plafond de diff sans appel, un appel par poussée, annulation des exécutions périmées, délai de 10 minutes, `max_tokens`, plafond de dépense de l'espace de travail `hearth-ci` à poser à la main. |
| Un autre contributeur avec droit d'écriture pousse une branche portant un flux modifié | Un flux d'une branche poussée est celui de la branche : le filtre d'acteur du flux ne protège donc pas contre quelqu'un qui peut écrire. Parades : à ce jour le propriétaire est seul à écrire ; ajouter à la règle de fédération la condition `actor=Voikyrioh` (voir le runbook) ; plafond de dépense de l'espace de travail ; le compte de service n'a accès qu'à l'espace `hearth-ci`. |
| Jeton OIDC ou d'accès volé dans les journaux | Masqués dès réception, jamais imprimés ; durée de vie de l'ordre de dix minutes. |
| Hausse de privilège du compte de service | Rôle `workspace:developer` (le même accès qu'une clé d'espace de travail) ; à restreindre si la console le permet. |

## Alternatives écartées
- **Action officielle de Claude Code** : non couverte par les crédits d'API mensuels du propriétaire.
- **Clé d'API en secret du dépôt** : un secret à garder, à faire tourner, qu'un flux détourné pourrait lire. Repli seulement si la fédération devenait indisponible : secret placé dans un environnement GitHub protégé (relecteur obligatoire, branche autorisée), et flux limité aux PR du dépôt lui-même ; non mis en service.
- **`pull_request` / `pull_request_target`** : la règle de fédération les refuse ; `pull_request_target` donnerait en plus des droits en écriture et des secrets à du code non fiable.

## Limites
- Calcy ne lit que le diff : il n'exécute rien, ne voit pas le reste du dépôt, peut se tromper ou manquer un défaut. Il ne remplace ni la CI (fmt, clippy, tests, `br-check`) ni la revue humaine.
- Une injection réussie peut fausser le verdict ou le contenu du commentaire (par exemple un faux « APPROUVER ») ; elle ne peut ni agir, ni publier autre chose que du texte inerte. Le commentaire est un avis, jamais une décision ni une condition de fusion.
- Le contenu du diff part chez l'API : ne jamais pousser de secret, même pour « voir ce que Calcy en dit ». Les branches poussées dans un dépôt public sont de toute façon publiques.
- Le coût réel dépend de la taille des jetons (l'estimation avant appel utilise 3 caractères par jeton, non vérifié pour le découpage du modèle actuel) ; seul le plafond de l'espace de travail le borne durement.
- Non vérifiable sans mise en service : l'échange réel depuis un flux (essai manuel du 09/10 réussi sur une branche jetable, pas sur ce flux), la condition d'acteur ajoutée à la règle, le comportement des permissions du jeton sur une vraie PR.

## Comment le couper
- Tout le flux : `gh workflow disable calcy-review.yml` (se réactive par `gh workflow enable`).
- Sans désactiver : `gh variable delete CALCY_MODEL` (le job ne démarre plus).
- Un commit : `[skip calcy]` dans le message du dernier commit poussé ; une PR : la passer en brouillon.
- Côté Anthropic : désactiver la règle de fédération ou le compte de service.

## Vérification
Essai à blanc, sans réseau, sans git, sans jeton (Node 20 ou plus) : `node .github/scripts/calcy-review.mjs --dry-run --diff .github/calcy/exemples/exemple.diff --review .github/calcy/exemples/exemple-review-hostile.md` affiche le découpage du diff, la taille du message, l'estimation de jetons et le commentaire tel qu'il serait posté après nettoyage de la review d'exemple (conçue pour contenir mention, image, lien, HTML et commande `/`).
