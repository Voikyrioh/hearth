---
id: ADR-0045
titre: Un seul modèle pour toutes les opérations de service : journal d'intention écrit avant chaque geste destructif, reprise qui ne rejoue rien, verrou partagé, décisions demandées au compte
type: architecture
statut: proposée (non validée par Voiky ; posée avec HRT-83, moitié agent du socle)
date: 2026-10-13
portee: repo
remplace: les tables `service_actions` et `deployments` de la migration `0009` (jamais écrites)
liens: [ADR-0031, ADR-0032, ADR-0033, ADR-0034, ADR-0038, ADR-0044, BR-LIFE-013, BR-EDIT-031, BR-DEL-019, BR-DEL-025, BR-BACKUP-025, BR-JEU-006, BR-JEU-011, BR-MONDE-026, BR-FILES-024, BR-RESIL-009, BR-RESIL-010, HRT-83, HRT-64, HRT-68]
---

> Posée par HRT-83, branchée par HRT-144 (2026-10-14). Non validée par Voiky. L'acte `DeploymentCancel` prévu par le lot 1 est devenu `OperationCancel` (`0x10`), avec `OperationDecide` (`0x1D`) : leurs routes existent (voir « Révision HRT-144 » en fin de fichier). Les codes d'acte et d'erreur des tranches L1 à L3 sont déclarés d'un coup.

# ADR-0045 : Journal des opérations de service

## Contexte
Démarrer, déployer, modifier, supprimer, sauvegarder, restaurer, installer par fiche, reprendre un monde : des gestes longs, dont la plupart détruisent quelque chose en route. Les specs exigent pour tous la même chose : rien n'est rejoué seul, l'issue est connue après une coupure, un redémarrage de l'agent en plein geste laisse un état dit. La migration `0009` avait deux tables propres à deux genres, sans étapes, qu'aucun code n'écrit.

## Décision
1. **Une table `service_ops` et un journal `service_op_steps`.** Dix genres fermés, onze statuts fermés. `last_action` et `DeploymentItem` du fil en seront des projections (sans changer de forme) : les tickets qui écrivent les actes les branchent.
2. **Plan figé à l'acceptation** (`plan_json`, jamais un secret) : les étapes dans l'ordre, lesquelles sont destructives, le **point de non-retour** du genre. Les étapes elles-mêmes sont déclarées par le ticket de chaque genre ; ce socle ne contient aucune opération réelle.
3. **Intention avant le geste.** Pour une étape destructive, la ligne `intent` (avec ce qu'il faut pour la défaire : les objets créés, jamais un secret) est écrite et validée SUR LE DISQUE avant le geste (`synchronous = FULL` le temps de la transaction, la base tournant sinon en WAL `NORMAL`), la ligne `done` après. Le service d'application refuse un `done` sans `intent` et une écriture sur une opération terminée.
4. **Au démarrage de l'agent**, `domain::ops::recover` (fonction pure, testée en table) classe chaque opération non terminée :
   - aucune étape destructive n'a de ligne : `interrupted`, « non exécuté » ;
   - `intent` sans `done`, ou étape échouée : `needs_attention`, verrou du service **repris**, choix offerts au compte ;
   - tout `done`, un seul état complet (ancien ou nouveau) : `interrupted`, avec le côté où tourne le service ;
   - tout `done`, deux états ou moteur illisible : `needs_attention`.
   **La reprise n'empêche JAMAIS l'agent de démarrer** (c'est l'outil qui répare le serveur). Un journal d'étapes illisible (ligne qui ne se relit pas : agent plus récent puis retour du binaire, base éditée) classe l'opération `needs_attention`, cause `state_unknown`, verrou repris, rien retiré ni rejoué, sans consulter le moteur. Une ligne d'OPÉRATION illisible est isolée (une trace avec l'identifiant et la colonne, jamais une valeur) et son service reste refusé par l'index unique ; un statut qui n'a pas pu s'écrire est tracé et l'agent démarre ; une liste d'opérations illisible dans son ensemble est tracée par la racine de composition, qui continue.
   **L'agent ne reprend, ne rejoue et ne défait jamais seul un geste sur un service ou ses données.** Le journal d'une opération classée n'est pas modifié. Seule exception : il retire les objets que l'opération avait créés pour elle-même (archive partielle, partie reçue, dossier d'import, dossier d'attente d'une restauration non commencée), nommés au journal, **revérifiés par leur numéro d'inode** (un fichier de même nom mais d'un autre inode n'est pas touché ; sans inodes, rien n'est retiré). Ce nettoyage précède la mise à jour du statut : s'il est interrompu, le démarrage suivant le rejoue, sans danger.
   Dans cette tranche l'observateur du moteur répond « illisible » : une opération destructive dont l'état n'est pas établi est « à vérifier », jamais supposée bonne.
5. **Verrou par service** en mémoire (table pure `domain::ops::LockTable`), trois modes : exclusif (toute opération), partagé en écriture (écritures de fichiers, refusées sous un exclusif), aucun (lectures, sauf la lecture de fichiers pendant une restauration). Deux verrous de serveur : `heavy_io` (une place) et `names` (nom réservé pendant une suppression ou un déploiement). Ceinture en base : index unique partiel sur `service_key` pour les opérations non terminées (tous les genres sont exclusifs). Le verrou est perdu au redémarrage, sauf pour une opération `needs_attention`, dont le verrou du service, le NOM réservé (suppression, déploiement) et la place des opérations lourdes (sauvegarde, restauration, reprise de monde) sont rebâtis au démarrage, déduits du genre (`OpKind::reserves_name`, `takes_heavy_io`) comme à l'acceptation.
6. **Décision demandée au compte** : statut `awaiting_decision` avec une liste fermée de choix ; une décision sans réponse expire en 15 minutes vers « annulé » (balayage toutes les 30 s, ce qu'elle avait créé pour elle-même est retiré) ; une opération `needs_attention` n'expire jamais ni n'est purgée. « Revenir » (`revert`) ne dit pas « revenu » : l'opération passe en `rolling_back`, verrou tenu, et ne devient `rolled_back` que par l'exécuteur du retour arrière (la transition `needs_attention` vers `rolled_back` n'existe pas). **L'acte `OperationDecide` (`0x1D`) et sa route `POST /operations/services/{id}/decision` existent (HRT-144).**
7. **Annulation** : chaque plan déclare son point de non-retour ; après lui, `cancel_verdict` rend `TooLate` (code d'erreur `TOO_LATE_TO_CANCEL`, 409). **L'acte `OperationCancel` (`0x10`) et sa route `DELETE /operations/services/{id}` existent (HRT-144).**
8. **Progression** : le message `operation` du sujet `services` (au plus un par 500 ms et par opération) est dans `ServicesMessage::Operation` (HRT-144). Ses types sont déclarés (`hearth_proto::api::service_ops`), tolérants aux valeurs inconnues.

## Comment l'appliquer
- Une opération nouvelle = un genre (déjà listé), un plan (étapes typées, destructives, point de non-retour), l'observateur du moteur qui dit « un état complet, lequel » ou « ambigu », et un test qui la tue à chaque étape.
- Les refus connus d'avance restent synchrones et typés (ADR-0038) ; l'opération n'existe qu'une fois acceptée.
- Aucun secret dans `plan_json` ni `undo_json`.

## Quand NE PAS l'appliquer / limites
- Livrés par HRT-144 (étaient reportés) : l'écriture de l'`intent` et l'annulation sont atomiques (la condition de statut est dans l'instruction qui écrit une ligne du journal d'étapes ; l'annulation juge le point de non-retour et change le statut dans la transaction de son entrée du journal d'activité) ; une décision expirée ou une opération annulée retire ce qu'elle avait créé pour elle-même (après le changement de statut). Reste reporté : `op_key` n'est pas encore consultée par le journal (le rejeu d'une requête suivie est rendu par la couche des opérations suivies, `Idempotency-Key`, ADR-0034) ; retirer un conteneur « créé jamais démarré » attend le port d'écriture du moteur (HRT-68).
- Un acte court et borné (régler une limite, enregistrer un fichier) reste synchrone ; il prend le verrou mais n'écrit pas d'étapes.
- Les opérations terminées sont purgées après 24 h ; une opération `needs_attention` jamais.
- Pas de résolution de chemin relative à un descripteur dans cette tranche : le retrait d'un objet créé vérifie l'inode puis supprime par chemin. La couche `SafeFs` (HRT-85) reprendra ce retrait.

## Alternatives rejetées
- **Garder `service_actions` et `deployments`, ajouter une table par genre** : six modèles, six reprises, verrous incohérents.
- **Reprise automatique au démarrage** (finir ou défaire) : interdit par les specs ; un dossier peut avoir été réutilisé entre-temps.
- **Marqueur dans un fichier par opération** (modèle de la mise à jour de l'agent) : pas de transaction avec la description.
- **Amender `0009` avant sa fusion** : rouvre une PR approuvée pour le même résultat.

## Conséquences
- Migration `0012` : supprime deux tables (elle REFUSE de s'appliquer si l'une contenait une ligne, sans rien détruire), en crée quatre avec `service_specs` et `service_spec_secrets`.
- HRT-64 et HRT-68 se développent sur ce modèle.
- Les actes `OperationCancel` et `OperationDecide`, avec `ActKind::ALL`, `covered_by_elevation`, `deploy/e2e/admin_act.py` et la table des routes, arrivent avec la première route qui les sert (aucune route n'est ajoutée ici).

## Références
- `crates/hearth-agent/src/domain/ops/`, `application/service_ops.rs`, `infrastructure/sqlite/ops_repo.rs`, `infrastructure/created_objects.rs`, `migrations/0012_service_specs_and_ops.sql`.
- Précédent : le marqueur d'étape de la mise à jour de l'agent (`application/update.rs`, `domain/update`).

## Révision HRT-144 (2026-10-14) : le socle est branché

- **Routes** (`/api/v1`) : `GET /operations/services/{id}` (tout compte authentifié, 24 heures après la fin ; une opération sur un service que le compte ne voit pas est `404 OPERATION_NOT_FOUND`, comme une inconnue, BR-OPS-001), `DELETE /operations/services/{id}` (acte `OperationCancel`, BR-OPS-002), `POST /operations/services/{id}/decision` (acte `OperationDecide`, BR-OPS-003). Annuler et décider exigent le droit « gérer les services » (ADR-0041) et la confirmation complète de l'acte.
- **Actes déclarés d'un coup** : `ServiceStart 0x0C`, `ServiceStop 0x0D`, `ServiceRestart 0x0E`, `ServiceDeploy 0x0F`, `OperationCancel 0x10`, `ServiceDelete 0x14`, `BackupCreate 0x19`, `BackupDelete 0x1B`, `OperationDecide 0x1D`, `GameSettingsSave 0x1E` (`ActKind::DECLARED_WITHOUT_ROUTE` liste ceux dont la route n'existe pas encore : aucune requête ne peut les produire). Annuler et décider sont couverts par l'élévation ; la décision SIGNE son choix.
- **Journal d'activité** : la réussite d'une annulation ou d'une décision s'écrit sous l'action du GENRE de l'opération (`service.start`, `service.deploy`, …), dans la transaction du changement, avec le compte ; un refus ou un échec de la route s'écrit sous `operation.cancel` ou `operation.decide` (le genre n'est pas connu avant d'avoir lu l'opération).
- **Flux** : `ServicesMessage::Operation(OperationView)` à l'abonnement (opérations en cours que le compte voit) puis à chaque changement ; cadence de 500 ms par opération et par connexion, état final jamais perdu (BR-OPS-004) ; `ServiceItem.busy` et `last_action` sont des projections du journal (BR-OPS-005).
- **Capacité** `service-ops` annoncée par `/hello` : lire, décider, annuler et suivre. Rien d'autre n'est annoncé (ni démarrer, ni déployer).
- **Bornes** : 500 opérations terminées gardées au plus (BR-OPS-007) ; canal de diffusion de 256 changements ; 500 opérations en cours rendues à un abonné ; cadence de 256 opérations par connexion.
- **Reprise** : l'observateur du vrai moteur compte les conteneurs du service (BR-OPS-008) ; `UnobservedState` ne sert plus qu'aux tests. Un conteneur « créé jamais démarré » n'est pas encore retiré (le port du moteur n'écrit pas) ; les dossiers sont retirés par l'adaptateur réel `FsCreatedObjects`, dont les racines (`<dossier de données>/backups`, `<dossier de données>/staging`) sont les défauts du plan d'intégration.
- **Tolérance** : `ErrorCode::Unknown` et `ServicesMessage::Unknown` sont les variantes de repli des deux sens ; un client ancien lit un code d'erreur ou un message qu'il ne connaît pas sans échouer.
