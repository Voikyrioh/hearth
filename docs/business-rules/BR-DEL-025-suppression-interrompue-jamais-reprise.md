---
id: BR-DEL-025
domaine: DEL
titre: Une suppression interrompue par un redémarrage n'est jamais reprise seule
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-DEL-025), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-13
---

# BR-DEL-025 — Une suppression interrompue par un redémarrage n'est jamais reprise seule

## Règle
Une suppression est une opération du journal : classée au démarrage comme toute autre (BR-EDIT-031). Une suppression ne se défait pas : la seule décision offerte à un compte est « j'ai vérifié ». L'effacement inachevé n'est jamais repris seul, le dossier pouvant avoir été réutilisé entre-temps. Le texte « des fichiers peuvent rester dans… » vient avec la suppression (HRT-85).

## Application (code)
- `crates/hearth-agent/src/domain/ops/kind.rs::OpKind::attention_choices`
- `crates/hearth-agent/src/domain/ops/recover.rs::recover`

## Vérification
- `crates/hearth-agent/src/domain/ops/kind.rs::tests::every_kind_takes_the_exclusive_lock_and_offers_the_verified_choice_when_in_doubt`
- `crates/hearth-agent/tests/service_ops_journal.rs::an_operation_to_verify_stays_so_across_restarts_and_waits_for_the_account`

## Cas limites
- Aucun geste d'effacement n'existe dans ce ticket.

## Règles liées
- BR-EDIT-031

## Historique
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
