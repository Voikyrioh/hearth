---
id: BR-FILES-019
domaine: FILES
titre: Un fichier envoyé arrive par morceaux de 4 Mio vérifiés un à un, n'est jamais visible sous son nom définitif tant qu'il est incomplet, et se reprend pendant 24 heures sur demande
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-fichiers-service.md (BR-FILES-019), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 7 et 9.2), contexts/hearth/conceptions/2026-10-13-adr-transferts-reprenables-brouillon.md, HRT-95
maj: 2026-11-15
---

# BR-FILES-019 : Un fichier envoyé arrive par morceaux de 4 Mio vérifiés un à un, n'est jamais visible sous son nom définitif tant qu'il est incomplet, et se reprend pendant 24 heures sur demande

## Règle
(a) **Un transfert = un WebSocket** (`GET /transfers/{id}`, premier message `auth`), ouvert par `POST /deposits` (acte `FileWrite` `0x16`, droit de gérer les services, couvert par l'élévation). (b) Des **morceaux de 4 Mio**, trame `[rang][SHA-256][données]` : l'agent vérifie le rang (le prochain, sans trou), la taille et l'empreinte, écrit puis rend durable (`fdatasync`), et ALORS accuse ; il ne lit une trame qu'après avoir écrit la précédente : **un morceau en mémoire** par transfert, rien pendant qu'un client est lent ou muet (60 secondes de silence ferment le socket). (c) À la fin, **l'empreinte de la suite des empreintes** de morceaux (`chunk_digest`) doit être celle que le client annonce ; la partie `<id>.part` est alors posée sous son nom définitif `<id>.bin` d'un seul geste (renommage qui ne remplace jamais), après avoir vérifié sa taille exacte : **le fichier n'est jamais visible sous son nom définitif tant qu'il est incomplet**. (d) La zone d'attente est un dossier de l'agent (`<données>/transfers`, `0700`), **hors de tout dossier de service et de tout montage**. (e) **La reprise se demande** (jamais seule) : un nouvel `attach` relit le DISQUE (liste d'empreintes, longueur de la partie), recoupe ce qui n'est pas durable et annonce l'offset ; les morceaux déjà reçus ne sont ni relus ni réécrits. Un nouveau socket prend la place de l'ancien. (f) **Vingt-quatre heures** après la dernière activité, la partie reçue (ou le fichier complet jamais consommé) est supprimée par l'agent ; c'est le SEUL fichier qu'il retire de la zone d'attente, toujours inscrit au registre `transfers`, toujours par son inode (ADR-0045). (g) Bornes : 4 transferts en cours pour l'agent, 2 par compte, 8 dépôts ouverts non terminés par compte (`TRANSFER_LIMIT`). (h) L'ouverture d'un dépôt puis la fin d'un envoi sont consignées sous `service.file.write` (compte, origine ; cibles `/deposits` et `/transfers/{id}`), jamais un nom de fichier ni un chemin ; l'agent ne garde aucun nom de fichier du client.

## Application (code)
- `crates/hearth-agent/src/domain/transfers/mod.rs::Receiver, ::admit`
- `crates/hearth-agent/src/application/transfers.rs::Transfers::{open_deposit, attach, abandon, purge_expired}, ::UploadSession::{put_chunk, finish}`
- `crates/hearth-agent/src/entrypoint/ws/transfers.rs`
- `crates/hearth-agent/src/entrypoint/http/transfers.rs`
- `crates/hearth-agent/src/infrastructure/transfer_files.rs::StdTransferFiles`
- `crates/hearth-agent/src/infrastructure/sqlite/transfers_repo.rs`
- `crates/hearth-proto/src/api/transfers.rs::chunk_digest`

## Vérification
- `crates/hearth-agent/tests/service_transfers_https.rs::a_file_of_several_chunks_arrives_verified_under_its_final_name_at_once`
- `crates/hearth-agent/tests/service_transfers_https.rs::an_interrupted_upload_resumes_without_rereading_what_was_received`
- `crates/hearth-agent/tests/service_transfers_https.rs::a_chunk_half_written_by_a_stop_is_overwritten_on_resume`
- `crates/hearth-agent/tests/service_transfers_https.rs::a_corrupt_chunk_is_refused_and_never_written`
- `crates/hearth-agent/tests/service_transfers_https.rs::the_end_needs_every_chunk_and_the_same_tree_digest`
- `crates/hearth-agent/tests/service_transfers_https.rs::a_part_not_resumed_in_24_hours_is_removed_and_nothing_else_is_touched`
- `crates/hearth-agent/tests/service_transfers_https.rs::several_gibibytes_cross_the_agent_without_being_held_in_memory`
- `crates/hearth-agent/tests/service_transfers_https.rs::the_agent_holds_at_most_two_transfers_per_account_and_four_in_all`
- `crates/hearth-agent/src/infrastructure/transfer_files.rs::tests (Linux)`
- `crates/hearth-proto/src/api/transfers.rs::tests`

## Cas limites
- Cette tranche livre l'envoi vers la zone d'attente. L'envoi vers un dossier de service (« Remplacer », « Garder les deux », verrou du service, propriétaire du fichier) vient avec l'onglet Fichiers ; le téléchargement (BR-BACKUP-026) vient après : le même socket et la même trame, dans le sens inverse.
- Entre deux morceaux, aucun descripteur n'est gardé : un client lent ne retient aucun fichier ouvert.
- Un arrêt de l'agent au milieu d'un morceau laisse des octets de trop dans la partie : ils sont écrasés à la reprise (la partie est tronquée à l'offset avant chaque morceau).
- Un morceau accusé dont les octets ont disparu du disque n'est plus compté à la reprise.
- Une trame de plus d'un morceau et 64 octets ferme le socket ; avant `auth`, une trame d'un morceau est lue en mémoire (au plus 12 connexions non authentifiées, un délai d'authentification court).
- Sous Windows (poste de développement), la zone d'attente répond « non disponible » : pas d'inode.

## Règles liées
- BR-FILES-013
- BR-FILES-023
- BR-FILES-005
- BR-MOD-015
- BR-GRANT-003

## Historique
- 2026-11-15 : création (HRT-95, côté agent, zone d'attente).
