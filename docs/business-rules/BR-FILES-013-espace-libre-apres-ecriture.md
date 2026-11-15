---
id: BR-FILES-013
domaine: FILES
titre: Avant toute écriture, il doit rester libre le plus grand de 1 Gio et de 5 % du disque ; la vérification est refaite pendant un envoi long
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-fichiers-service.md (BR-FILES-013), HRT-95
maj: 2026-11-15
---

# BR-FILES-013 : Avant toute écriture, il doit rester libre le plus grand de 1 Gio et de 5 % du disque ; la vérification est refaite pendant un envoi long

## Règle
Avant d'écrire un fichier reçu, Hearth vérifie qu'il restera, APRÈS l'écriture, au moins le plus grand de 1 Gio et de 5 % du disque libres. À l'ouverture d'un dépôt, pour TOUTE la taille annoncée (rien n'est créé si elle ne tient pas) ; puis avant CHAQUE morceau de 4 Mio reçu (un envoi est long, un autre programme remplit le disque). Sinon l'erreur est `507 NO_SPACE` (ou `NO_SPACE` sur le socket) avec `missing_bytes` (ce qui manque) et `free_after_bytes` (ce qu'il resterait) : le client en fait « Il manque {x} sur le disque pour écrire ce fichier. Il resterait {y} de libre. ». Le morceau refusé n'est pas écrit, les morceaux déjà reçus restent et l'envoi se reprend quand de la place revient. Aucune case ne permet de passer outre.

## Application (code)
- `crates/hearth-agent/src/domain/transfers/mod.rs::reserve, ::judge_space`
- `crates/hearth-agent/src/application/transfers.rs::Transfers::open_deposit, ::UploadSession::put_chunk`
- `crates/hearth-agent/src/entrypoint/http/transfers.rs::refusal`

## Vérification
- `crates/hearth-agent/src/domain/transfers/tests.rs::space_must_leave_the_larger_of_one_gib_and_five_percent`
- `crates/hearth-agent/tests/service_transfers_https.rs::a_file_that_does_not_fit_is_refused_before_anything_is_created`
- `crates/hearth-agent/tests/service_transfers_https.rs::the_disk_filling_up_mid_upload_stops_the_next_chunk`

## Cas limites
- La mesure est celle du disque de la zone d'attente (`<données>/transfers`) ; le disque d'un dossier de service viendra avec l'envoi vers un service.
- Entre la mesure et l'écriture, un autre programme peut remplir le disque : l'écriture échoue alors en `WRITE_FAILED`, sans fichier partiel visible sous un nom définitif.

## Règles liées
- BR-FILES-019
- BR-BACKUP-009

## Historique
- 2026-11-15 : création (HRT-95, côté agent, zone d'attente).
