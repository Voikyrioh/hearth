---
id: BR-UPDATE-029
domaine: UPDATE
titre: Un retour arrière ne laisse jamais un ancien binaire devant une base déjà migrée
statut: active
invariant: true
source: revue Stephen de HRT-17 (round 1), ADR-0014
maj: 2026-10-06
---

# BR-UPDATE-029 : Un retour arrière ne laisse jamais un ancien binaire devant une base déjà migrée

## Règle
Service arrêté, avant l'échange des binaires, le superviseur copie la base (`hearth.db`, et son journal s'il en reste un) dans `update/`. Un retour arrière (nouvel agent muet, autre certificat, reprise d'un travail orphelin) suit le plan `domain::update::rollback_plan` : arrêt, **base d'avant, puis ancien binaire**, redémarrage. La copie et la remise se font par un fichier voisin puis un renommage (jamais de base tronquée : un disque plein laisse la base vivante intacte) ; l'espace libre est contrôlé avant la copie (deux fois la taille de la base, plus 1 Mio) : **un espace insuffisant, ou impossible à mesurer, refuse l'échange** (jamais « assez de place » faute de mesure) ; **le contrôle se fait avant l'arrêt du service** : un refus ne coûte ni coupure ni redémarrage (`failed` / `swap`, dépôt nettoyé ; seul `state.json` avait été écrit). La place mesurée est celle d'un utilisateur ordinaire (`df`, `f_bavail`) : sur un disque dont il ne reste que la réserve de root, le refus tombe alors que la copie tiendrait. Prudent, assumé. **La base n'est remise qu'une fois**, dans le retour arrière qui remet l'ancien binaire, et la copie est retirée aussitôt : une reprise ultérieure qui trouve l'ancienne version en place (BR-UPDATE-028) ne recopie jamais une copie périmée. Le service retrouve exactement l'état d'avant. Une réussite retire la copie. Conséquence acceptée et dite : ce que le nouvel agent a écrit dans la base pendant la fenêtre de contrôle (comptes, journal) est perdu au retour arrière ; l'alternative était un ancien binaire qui refuse de démarrer (SQLx : migration inconnue) et un serveur sans agent.

## Application (code)
- `crates/hearth-agent/src/domain/update/rollback.rs::rollback_plan` (la décision « base d'abord, binaire ensuite »).
- `crates/hearth-agent/src/domain/update/space.rs::{check_space, required_space}` (la décision : assez, pas assez, impossible à mesurer).
- `crates/hearth-agent/src/application/update_supervisor.rs::Supervisor::{run, roll_back, check_space}` (la mesure passe par le port `FreeSpace`).
- `crates/hearth-agent/src/infrastructure/update/host.rs::FsUpdateHost::{backup_database, restore_database}`.

## Vérification
- `domain::update::space::tests` (les trois cas), `tests/update_supervisor.rs` : `not_enough_room_for_the_database_copy_leaves_everything_as_it_was`, `a_room_that_cannot_be_measured_refuses_before_any_swap_like_a_full_disk` (espace simulé : aucun test ne lit le vrai disque), `domain::update::rollback::tests`, `infrastructure::update::host::tests` (copie atomique, remise une seule fois, remise impossible : base vivante intacte), `tests/update_use_cases.rs::the_old_version_already_running_with_the_traces_left_is_concluded_without_touching_the_database` (le scénario de la reprise à la main).
- `tests/update_supervisor.rs` : `a_rollback_puts_the_database_back_as_it_was_before_the_swap`, `a_success_keeps_the_migrated_database_and_drops_its_copy`, `a_recovery_puts_back_the_exact_old_binary_and_database_when_the_new_agent_does_not_hold`.

## Cas limites
- Sans base (première installation) : rien à copier ni à remettre.
- La copie se prend service arrêté : la base est fermée (journal vide).

## Règles liées
- BR-UPDATE-015, BR-UPDATE-018, ADR-0014

## Historique
- 2026-10-05 : création (HRT-17, suite de la revue de code).
- 2026-10-06 : l'espace impossible à mesurer est un refus, port `FreeSpace`, décision dans `domain/` (HRT-17, suivis de la review). La base n'est remise que par le retour arrière de CE travail (FIX-01M47N6Z485TWN2H770KQ5H80R : jamais sous une version posée à la main).
