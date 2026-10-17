---
id: BR-RELEASE-001
domaine: RELEASE
titre: Une version déposée au bucket ne se réécrit jamais
statut: active
invariant: true
source: HRT-51, contexts/hearth/conceptions/2026-10-09-distribution-scaleway.md (sections 4.7, 8.2), ADR-0035 (décision 5)
maj: 2026-10-12
---

# BR-RELEASE-001 : Une version déposée ne se réécrit jamais

## Règle
`cargo xtask release-upload --confirm` cherche, AVANT toute écriture, chacune des 8 clés `releases/X.Y.Z/*` dans le bucket (`aws s3api head-object`, puis relecture du contenu par `aws s3 cp s3://… -` et comparaison de la somme SHA-256 ; l'ETag n'est pas utilisé, il ne prouve rien pour un dépôt en plusieurs parties) :
- clé absente : elle sera écrite ;
- clé présente avec le MÊME contenu : elle est passée (une relance après une panne reprend où elle s'est arrêtée) ;
- clé présente avec un AUTRE contenu : refus et arrêt, sans qu'aucune écriture n'ait eu lieu (message exact de BR-RELEASE-009 : « {fichier} est déjà déposé avec un autre contenu : rien n'est écrasé. … ») ;
- état impossible à établir (403, délai, réseau, sortie inattendue) : arrêt aussi, avant toute écriture, avec un message qui dit quoi vérifier. Jamais « absente par défaut ». L'absence n'est reconnue QUE par le code de sortie 254 d'`aws` (erreur de service) ET une ligne de sa sortie d'erreur exactement égale à `An error occurred (404) when calling the HeadObject operation: Not Found`. Un texte qui contient « 404 » ou « not found » ailleurs (par exemple un délai dépassé qui cite une clé en `1.404.0`) n'est PAS une absence.
- avant toute écriture aussi, le manifeste en place (`releases/manifest.json`) est lu : s'il annonce une version (client ou agent) SUPÉRIEURE à celle déposée, REFUS, sauf `--allow-rollback` (retour arrière voulu ; il laisse alors une ligne `RETOUR ARRIÈRE : le manifeste en place annonce …, il sera remplacé par X.Y.Z` avant la première écriture, et l'essai à blanc dit que l'option est armée). Illisible, même avec l'option : arrêt. Absent : premier dépôt, accepté.

Pourquoi : la signature se fait avant l'étiquette Git `vX.Y.Z`. Entre les deux, `client-release-check` ne voit pas qu'un numéro a déjà été signé. Le refus d'écraser une version déjà déposée ferme cette fenêtre À PARTIR DU PREMIER OBJET DÉPOSÉ : un numéro signé puis déposé, même en partie, est brûlé et ne peut plus porter un autre binaire. Un numéro signé puis jamais déposé reste resignable (rien dans le bucket ne le contredit). Seuls `releases/manifest.json` et `releases/install.sh` (objets mutables) sont réécrits.

## Application (code)
- `xtask/src/release_publish.rs::publish` (phase 1a ; message `refuse_different` : l'état de toutes les clés avant toute écriture), `::remote_state`, `::fetch`, `::proves_absence`, `::check_not_older` (manifeste en place).

## Vérification
- `xtask/src/release_publish.rs` : `a_key_already_there_with_another_content_is_refused_before_any_write`, `a_key_already_there_with_the_same_content_is_skipped`, `a_rerun_after_a_complete_deposit_writes_only_the_two_mutable_objects`, `an_absent_key_answering_403_stops_before_any_write_and_says_what_to_check`, `a_timeout_whose_text_contains_404_is_not_an_absence`, `a_not_found_outside_a_404_is_not_an_absence`, `the_exact_404_line_alone_is_a_proof_of_absence`, `the_prefixed_404_line_of_a_recent_aws_cli_is_an_absence`, `only_the_exact_prefix_is_accepted_before_the_404_line`, `a_read_failing_after_a_successful_head_stops_before_any_write`, `a_write_failing_in_the_middle_leaves_a_clean_state_and_the_rerun_resumes`, `a_manifest_in_place_announcing_a_newer_version_is_refused_before_any_write`, `the_explicit_rollback_option_lets_an_older_version_replace_the_manifest`, `an_equal_older_or_absent_manifest_in_place_is_accepted_and_an_unreadable_one_stops`, `the_rollback_option_does_not_excuse_an_unreadable_manifest_in_place`, `the_dry_run_says_when_the_rollback_option_is_armed`.

## Cas limites
- Ne protège pas d'une écriture concurrente entre la recherche et le dépôt (un seul propriétaire dépose) ; le versionnement du bucket garde la trace de tout remplacement (conception 4.7). Piste, NON retenue ici : une écriture conditionnelle (`If-None-Match: *`, `aws s3api put-object --if-none-match '*'`) ferme cette course, l'absence étant alors prouvée par le stockage au moment de l'écriture ; sa prise en charge par Scaleway n'est PAS vérifiée.
- La phrase d'absence et le code 254 sont ceux de l'AWS CLI v2 ; non vérifiés contre Scaleway : à relever au premier dépôt d'essai (clé absente : texte exact et code de sortie). Si Scaleway répond autrement, l'outil s'arrête (échec sûr) et la constante `ABSENT_PHRASE` est à corriger.
- Droits requis de la clé de publication : LECTURE d'objet (`s3:GetObject`, nécessaire à `head-object` et à la relecture) et ÉCRITURE (`s3:PutObject`, dépôts en plusieurs parties compris), SANS suppression. Sans droit de lister le bucket, une clé absente répond 403 et non 404 : l'outil s'arrête alors (« état impossible à établir »). À vérifier à la console AVANT le premier dépôt.
- Bucket versionné : une version supprimée à la console (marqueur de suppression) redevient « absente » pour l'outil et pourrait être redéposée avec d'autres octets. « Un numéro déposé est brûlé » ne tient que tant que personne ne supprime : ne jamais supprimer une version déposée. Non vérifié chez Scaleway.
- La clé de publication n'a pas le droit de supprimer : retirer une version se fait à la console, hors de cet outil.

## Règles liées
- BR-RELEASE-002, ADR-0035

## Historique
- 2026-10-12 : création (HRT-51).
- 2026-10-17 : message du refus aligné sur la spec (HRT-174, BR-RELEASE-009).
- 2026-10-12 : correctifs de la review de la PR 73 (round 1).
- 2026-10-12 : round 2 : retour arrière accepté signalé par une ligne, cas limite du bucket versionné, liste des tests corrigée.
