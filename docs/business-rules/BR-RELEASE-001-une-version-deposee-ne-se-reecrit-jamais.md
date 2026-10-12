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
- clé présente avec un AUTRE contenu : REFUS et arrêt, sans qu'aucune écriture n'ait eu lieu ;
- état impossible à établir (droit manquant, réseau) : arrêt aussi, jamais « absente par défaut ».

Pourquoi : la signature se fait avant l'étiquette Git `vX.Y.Z`. Entre les deux, `client-release-check` ne voit pas qu'un numéro a déjà été signé. Le refus d'écraser une version déjà déposée ferme cette fenêtre : un numéro signé puis déposé, même en partie, est brûlé et ne peut plus porter un autre binaire. Seuls `releases/manifest.json` et `releases/install.sh` (objets mutables) sont réécrits.

## Application (code)
- `xtask/src/release_publish.rs::publish` (phase 1a : l'état de toutes les clés avant toute écriture), `::remote_state`.

## Vérification
- `xtask/src/release_publish.rs` : `a_key_already_there_with_another_content_is_refused_before_any_write`, `a_key_already_there_with_the_same_content_is_skipped`, `an_unreadable_bucket_stops_before_any_write_and_says_what_to_check`, `a_rerun_after_a_complete_deposit_writes_only_the_two_mutable_objects`.

## Cas limites
- Ne protège pas d'une écriture concurrente entre la recherche et le dépôt (un seul propriétaire dépose) ; le versionnement du bucket garde la trace de tout remplacement (conception 4.7).
- La clé de publication n'a pas le droit de supprimer : retirer une version se fait à la console, hors de cet outil.

## Règles liées
- BR-RELEASE-002, ADR-0035

## Historique
- 2026-10-12 : création (HRT-51).
