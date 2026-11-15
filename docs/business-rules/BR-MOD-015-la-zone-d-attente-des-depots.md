---
id: BR-MOD-015
domaine: MOD
titre: Un fichier déposé par un compte arrive dans une zone d'attente de l'agent, hors de tout dossier de service, et n'est rangé qu'après le contrôle de son consommateur ; abandonné, il est retiré au bout de 24 heures
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-mods-et-packs.md (BR-MOD-015), HRT-95, HRT-124
maj: 2026-11-15
---

# BR-MOD-015 : Un fichier déposé par un compte arrive dans une zone d'attente de l'agent, hors de tout dossier de service, et n'est rangé qu'après le contrôle de son consommateur ; abandonné, il est retiré au bout de 24 heures

## Règle
Partout où Hearth attend un fichier de l'utilisateur pour un service (pack de mods, archive de mods, archive d'un monde), le fichier arrive par un transfert reprenable (BR-FILES-019) dans la **zone d'attente de l'agent** : `<données>/transfers`, `0700`, hors de tout dossier de service et de tout montage. Il n'est rangé avec le service qu'APRÈS le contrôle du consommateur (lecture d'archive hostile, ADR-0055). Le consommateur passe par le port `DepositZone` : `take(id, compte)` rend le fichier COMPLET en lecture seule et déplaçable (jamais un chemin), SEULEMENT pour le compte qui l'a déposé et SEULEMENT si c'est encore le fichier que Hearth a posé (inode, taille, date) ; `release(id, compte)` le retire quand le consommateur a fini (un autre compte : `NotFound`) (rangé ou refusé). Abandonné, un dépôt est retiré 24 heures après sa dernière activité, complet ou non. Cette fiche couvre la moitié agent de BR-MOD-015 (la zone d'attente, la reprise, les 24 heures, la propriété d'un dépôt) ; la zone de dépôt de l'écran (un seul fichier, taille maximale d'un pack, textes) est du client. Le dépôt exige le droit de gérer les services.

## Application (code)
- `crates/hearth-agent/src/application/transfers.rs::DepositZone, ::Transfers::take, ::Transfers::release`
- `crates/hearth-agent/src/application/transfers.rs::Transfers::purge_expired`
- `crates/hearth-agent/src/entrypoint/tasks.rs::spawn_transfers_purge`
- `crates/hearth-agent/src/app.rs::TRANSFERS_DIR`

## Vérification
- `crates/hearth-agent/tests/service_transfers_https.rs::the_consumer_reads_the_complete_deposit_then_releases_it`
- `crates/hearth-agent/tests/service_transfers_https.rs::a_complete_deposit_replaced_by_hand_is_never_handed_to_the_consumer`
- `crates/hearth-agent/tests/service_transfers_https.rs::another_account_never_sees_attaches_or_abandons_a_deposit`
- `crates/hearth-agent/tests/service_transfers_https.rs::an_expired_deposit_cannot_be_resumed_and_a_complete_one_unconsumed_goes_too`

## Cas limites
- Un consommateur qui plante avant `release` : le dépôt part de lui-même au bout de 24 heures.
- Le contrôle (lecture d'archive, genre du pack) n'est pas dans cette tranche : le consommateur le fait sur le lecteur rendu par `take`.
- Un fichier étranger posé dans la zone n'a pas de ligne au registre : jamais touché, jamais retiré.

## Règles liées
- BR-FILES-019
- BR-ARCH-007
- BR-GRANT-003

## Historique
- 2026-11-15 : création de la moitié agent (HRT-95).
