---
id: BR-MOD-018
domaine: MOD
titre: Un dépôt est retiré du serveur dès qu'une installation l'a consommé, ou après 24 heures sans usage ; la liste Transferts du client retire d'elle-même un envoi réussi dont le fichier n'y est plus
statut: active
invariant: false
source: HRT-222 (smoke du 16/11/2026), BR-MOD-015, HRT-96, HRT-124
maj: 2026-11-16
---

# BR-MOD-018 : Un dépôt consommé ou expiré quitte la zone d'attente et la liste Transferts

## Règle
- **Côté agent** (BR-MOD-015, déjà portée) : un dépôt (pack, archive de mods, archive d'un monde) est retiré de la zone d'attente DÈS que son consommateur l'a rangé avec succès (`PackDeposits::place` puis `DepositZone::release`) ou refusé (`discard`). Un dépôt que personne n'a consommé (installation abandonnée ou en échec avant le rangement) est retiré 24 heures après sa dernière activité, complet ou non : au démarrage de l'agent, puis toutes les 10 minutes. Le pack RANGÉ dans le dossier `packs` du service n'est pas un dépôt : c'est une donnée du service (le jeu le relit, le pack se redépose pour le remplacer), il suit la vie du service.
- **Côté client** : la liste Transferts retire d'elle-même un envoi RÉUSSI dont le fichier n'est plus sur le serveur (`GET /deposits/{id}` répond introuvable ou expiré). Le serveur est relu en tâche de fond, au plus toutes les 30 secondes par envoi réussi, jamais pour une réponse de liste. Une réponse qui ne dit rien (lien coupé, refus d'un autre ordre) laisse la liste telle quelle. Un envoi échoué, annulé ou interrompu n'est jamais retiré sans geste : le compte le retire avec « Retirer de la liste ». Le geste « Retirer de la liste » reste disponible pour tout envoi fini.

## Application (code)
- `crates/hearth-agent/src/infrastructure/pack_deposits.rs::ZonePackDeposits::{place, discard}`
- `crates/hearth-agent/src/application/transfers.rs::Transfers::{release, purge_expired}`
- `crates/hearth-agent/src/entrypoint/tasks.rs::spawn_transfers_purge`
- `apps/desktop/src-tauri/src/transfers/service.rs::{reconcile, gone_from_server, TransferBook::{due_for_check, forget_gone}}`
- `apps/desktop/src-tauri/src/transfers/commands.rs::list_transfers`

## Vérification
- `crates/hearth-agent/tests/pack_zone_deposits.rs::a_complete_deposit_is_read_placed_and_released`
- `crates/hearth-agent/tests/pack_install_https.rs::a_server_pack_is_stored_under_a_fixed_name_in_a_read_only_folder_and_its_path_goes_to_the_game`
- `crates/hearth-agent/tests/service_transfers_https.rs::an_expired_deposit_cannot_be_resumed_and_a_complete_one_unconsumed_goes_too`
- `apps/desktop/src-tauri/src/transfers/service.rs::tests::a_finished_upload_whose_file_left_the_server_leaves_the_list_by_itself`

## Cas limites
- Le dépôt est un fichier de l'utilisateur, jamais un chemin du serveur : la liste ne montre ni l'emplacement ni l'espace occupé (pas de ligne d'échéance dans cette tranche).
- Un redémarrage de l'application oublie la liste (rien n'est écrit sur disque) : la purge de 24 heures du serveur reste le filet.

## Règles liées
- BR-MOD-015, BR-FILES-019

## Historique
- 2026-11-16 : création (HRT-222). La rétention de 24 heures et la consommation étaient déjà portées côté agent ; la liste du client ne s'en apercevait pas.
