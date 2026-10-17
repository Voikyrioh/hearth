# Architecture — domain/ops

Les opérations de service : un seul modèle pour tous les gestes longs et destructifs (HRT-83, ADR-0045). Règles pures, sans moteur, sans base, sans horloge système. Le journal vit dans `application/ports/op_journal.rs` (adaptateur `infrastructure/sqlite/ops_repo.rs`), le cas d'usage dans `application/service_ops.rs`.

```
ops/
├── kind.rs     → `OpKind` (dix genres), `OpStatus` (onze statuts, `can_become` = le diagramme d'états, `holds_lock`), `Choice`, `Cause` : listes fermées, mêmes noms que sur le fil (`hearth_proto::api::service_ops`) ; `attention_choices` par genre (une suppression ne se « revient » pas)
├── plan.rs     → `OpPlan` (étapes dans l'ordre, lesquelles sont destructives, point de non-retour), `StepName` (code stable), `StepRecord`/`StepState` (le journal : `intent` AVANT le geste, `done` après), `CreatedObject`/`UndoInfo` (ce que l'opération a créé pour elle-même : racine connue, chemin relatif sans `..`, inode ; jamais un secret)
├── recover.rs  → `recover` : le classement au démarrage de l'agent, en table, sans rien rejouer ni défaire ; les objets créés à retirer (archive partielle, partie reçue, dossier d'import, dossier d'attente d'une restauration non commencée) — BR-EDIT-031, BR-BACKUP-025, BR-MONDE-026
├── decision.rs → `cancel_verdict` (avant le point de non-retour, sinon `TOO_LATE_TO_CANCEL`), `apply_choice`, `decision_expired` (15 minutes) — BR-JEU-006
├── lifecycle.rs → démarrer, arrêter, redémarrer (HRT-64) : `Gesture` (trois gestes, plan d'UNE étape non destructive), `check_state` (le geste est-il permis dans cet état : déjà atteint, non pilotable), `engine_ready`, `wait_budget` (délai d'arrêt + 30 s, au moins 60 s), `engine_stop_was_forced` (137 après le délai), `supervised_stop_was_forced`, `STARTUP_WATCH` (10 s) et `RESTART_WATCH` (30 s) — BR-LIFE-003, 005, 009, 011, 014, 019, 021
├── access.rs   → `ServiceOrigin`, `origin_of`, `may_see` : qui voit l'opération d'un service (créé par Hearth : tous ; adopté : le droit ; autre : personne, réponse d'une opération inconnue) — BR-OPS-001 (HRT-144)
├── flush.rs    → `OpFlush` : la cadence des messages `operation` d'une connexion (500 ms par opération, le dernier état retenu part à l'échéance, borné à 256 opérations) — BR-OPS-004
├── index.rs    → `OpsIndex`, `OpSummary` : `busy` et `last_action` du fil, projections du journal, bornées — BR-OPS-005
├── observe.rs  → `Presence`, `observed_from_presence` : le décompte des conteneurs du moteur devient l'`Observed` que `recover` classe, sans jamais présumer un état cohérent — BR-OPS-008
└── locks.rs    → `LockTable` : exclusif / partagé en écriture / aucun par service, noms réservés, `heavy_io` ; rebâtie au démarrage pour les opérations « à vérifier » — BR-LIFE-013, BR-DEL-019, BR-FILES-024
```

`decision.rs` et `recover.rs` portent aussi `cleanup_on_cancel` (ce qu'une opération annulée ou dont la décision a expiré avait créé pour elle-même). Les routes, le journal d'activité et le flux sont dans `application/service_op_access.rs` et `entrypoint/{http/service_ops.rs, service_ops_wire.rs, ws/connection.rs}` (HRT-144).

Règle d'or : **l'agent ne reprend, ne rejoue et ne défait jamais seul un geste sur un service ou ses données.** Un genre de plus = un genre dans `OpKind` (et dans la migration), son plan, son observateur du moteur, et un test qui le tue à chaque étape.
