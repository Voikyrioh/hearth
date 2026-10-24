---
id: BR-DEL-029
domaine: DEL
titre: L'entrée du journal d'activité d'une suppression ne porte jamais de chemin, de nom de fichier, de variable ni de commande
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-029), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §5 et §9, HRT-85
maj: 2026-10-24
---

# BR-DEL-029 : L'entrée du journal d'activité d'une suppression ne porte jamais de chemin, de nom de fichier, de variable ni de commande

## Règle
Chaque suppression demandée est inscrite au journal d'activité : compte, action `service.delete`, nom du service, mentions « données effacées (n dossiers, taille) », « arrêt forcé », « données non effacées (n) », résultat et cause typée dans la liste fermée (arrêt forcé, moteur inaccessible, service introuvable, droits insuffisants, action déjà en cours, service adopté, refusé par le moteur, interrompu par un redémarrage de l'agent, délai dépassé). Le type `Target::ServiceDelete` n'a aucun champ pour un chemin, un nom de fichier, une variable ou une commande ; la trace de fin ne dit que l'opération. Annuler ou refuser avant l'acceptation n'écrit que le refus. Test sentinelle : nom de dossier, valeur et nom de variable, commande absents du journal, des traces et des fichiers de l'agent.

## Application (code)
- `crates/hearth-agent/src/domain/audit/event.rs::{Target::ServiceDelete, DeleteNote, ErasedData}`
- `crates/hearth-agent/src/application/delete.rs::ServiceDeletion::{draft, settle}`

## Vérification
- `crates/hearth-agent/tests/service_delete_https.rs::no_path_folder_name_or_secret_reaches_the_journal_the_traces_or_the_operation`
- `crates/hearth-agent/src/domain/audit/event.rs::delete_tests`

## Cas limites
- La taille effacée est mesurée avant l'effacement, dans le budget de 10 s ; « plus de » quand la mesure a été coupée.

## Règles liées
- BR-DEL-030
- BR-DEL-016

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
