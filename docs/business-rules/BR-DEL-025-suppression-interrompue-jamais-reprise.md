---
id: BR-DEL-025
domaine: DEL
titre: Une suppression interrompue par un redémarrage n'est jamais reprise seule
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-DEL-025), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-24
---

# BR-DEL-025 — Une suppression interrompue par un redémarrage n'est jamais reprise seule

## Règle
Une suppression est une opération du journal : classée au démarrage comme toute autre (BR-EDIT-031). Une suppression ne se défait pas : la seule décision offerte à un compte est « j'ai vérifié ». L'effacement inachevé n'est jamais repris seul, le dossier pouvant avoir été réutilisé entre-temps. Chaque étape qui détruit de la suppression (`remove_container`, `forget`, `erase`) écrit son `intent` avant le geste : un agent qui redémarre au milieu laisse une opération `needs_attention` (cause `intent_without_done`), verrou et NOM repris (un déploiement du même nom reste refusé), seule décision « j'ai vérifié ». Il en va de même d'une PANIQUE de l'exécution après l'`intent` d'un de ces gestes (`ServiceOps::park_for_check`, la même décision que `recover`) : `needs_attention`, nom réservé, jamais `failed` (« le service reste entier » serait faux). Une coupure ou une panique pendant l'arrêt, avant le point de non-retour, laisse le service entier : l'opération est `interrupted`. Le texte « des fichiers peuvent rester dans {chemins} » est celui du client (HRT-86), qui connaît les chemins par la fenêtre de suppression ; l'agent ne les garde pas (ils ne sont pas au journal des opérations).

## Application (code)
- `crates/hearth-agent/src/domain/ops/kind.rs::OpKind::attention_choices`
- `crates/hearth-agent/src/domain/ops/recover.rs::recover`
- `crates/hearth-agent/src/domain/ops/delete.rs::op_plan` (les trois étapes qui détruisent)

## Vérification
- `crates/hearth-agent/src/domain/ops/kind.rs::tests::every_kind_takes_the_exclusive_lock_and_offers_the_verified_choice_when_in_doubt`
- `crates/hearth-agent/tests/service_ops_journal.rs::an_operation_to_verify_stays_so_across_restarts_and_waits_for_the_account`
- `crates/hearth-agent/src/domain/ops/delete.rs::tests::a_delete_cut_by_a_restart_is_never_resumed_and_offers_only_the_check`
- `crates/hearth-agent/tests/service_delete_https.rs::an_agent_restarting_in_the_middle_of_an_erasure_never_resumes_it_and_asks_the_account`
- `crates/hearth-agent/tests/service_delete_https.rs::a_panic_after_the_container_removal_order_leaves_the_operation_to_check_and_the_name_reserved`

## Cas limites
- L'ancien fil d'un agent coupé n'existe plus dans un vrai arrêt ; le test le simule par un faux dossier suspendu, qu'il relâche à la fin.

## Règles liées
- BR-EDIT-031

## Historique
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
- 2026-10-24 — la suppression existe (HRT-85) : plan, test de bout en bout.
- 2026-10-24 — revue de la PR 125 (round 2) : une panique après l'`intent` donne `needs_attention`, comme un redémarrage.
