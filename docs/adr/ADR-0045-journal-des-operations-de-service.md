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

> Posée par HRT-83. Non validée par Voiky. L'acte `DeploymentCancel` prévu par le lot 1 (code `0x10`) deviendra `OperationCancel` avec sa route (HRT-64) ; cette tranche ne déclare aucun acte ni aucune route.

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
6. **Décision demandée au compte** : statut `awaiting_decision` avec une liste fermée de choix ; une décision sans réponse expire en 15 minutes vers « annulé » (balayage toutes les 30 s) ; une opération `needs_attention` n'expire jamais ni n'est purgée. « Revenir » (`revert`) ne dit pas « revenu » : l'opération passe en `rolling_back`, verrou tenu, et ne devient `rolled_back` que par l'exécuteur du retour arrière (la transition `needs_attention` vers `rolled_back` n'existe pas). L'acte `OperationDecide` et sa route viennent avec le premier ticket qui demande une décision.
7. **Annulation** : chaque plan déclare son point de non-retour ; après lui, `cancel_verdict` rend `TooLate` (code d'erreur `TOO_LATE_TO_CANCEL`, 409). L'acte `OperationCancel` et sa route viennent avec leur premier appelant.
8. **Progression** : le message `operation` du sujet `services` (au plus un par 500 ms et par opération) rejoint `ServicesMessage` avec le suivi côté client (HRT-84). Ses types sont déclarés (`hearth_proto::api::service_ops`), tolérants aux valeurs inconnues.

## Comment l'appliquer
- Une opération nouvelle = un genre (déjà listé), un plan (étapes typées, destructives, point de non-retour), l'observateur du moteur qui dit « un état complet, lequel » ou « ambigu », et un test qui la tue à chaque étape.
- Les refus connus d'avance restent synchrones et typés (ADR-0038) ; l'opération n'existe qu'une fois acceptée.
- Aucun secret dans `plan_json` ni `undo_json`.

## Quand NE PAS l'appliquer / limites
- Reportés (ticket de suivi) : l'écriture de l'`intent` et l'annulation ne sont pas encore atomiques (la condition de statut et le point de non-retour doivent entrer dans l'instruction qui écrit, avec l'acte d'annulation) ; une décision expirée ne retire pas ce que l'opération avait créé ; `op_key` n'est pas encore consultée (rejeu).
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
