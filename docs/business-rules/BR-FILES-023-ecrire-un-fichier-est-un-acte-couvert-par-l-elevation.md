---
id: BR-FILES-023
domaine: FILES
titre: Parcourir et lire ne demandent que le droit ; envoyer un fichier est un acte d'administration couvert par l'élévation de 5 minutes ; supprimer, revenir en arrière et remplacer ne le sont jamais
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-fichiers-service.md (BR-FILES-023), HRT-95
maj: 2026-11-15
---

# BR-FILES-023 : Parcourir et lire ne demandent que le droit ; envoyer un fichier est un acte d'administration couvert par l'élévation de 5 minutes ; supprimer, revenir en arrière et remplacer ne le sont jamais

## Règle
Déposer un fichier (acte `FileWrite` `0x16`, geste `upload`) est un acte d'administration du socle : le droit de gérer les services, la preuve de clé du poste et le mot de passe du compte, **couvert** par l'élévation de 5 minutes pour qu'une session de réglage ne le redemande pas à chaque envoi. Cible signée : `deposit` (la zone d'attente). Paramètres signés : le geste, le dernier nom du fichier et sa taille ; jamais un chemin ni un octet de contenu. La confirmation se juge à l'OUVERTURE du dépôt (`POST /deposits`), pas à chaque morceau ; le socket revérifie seulement la session et le droit pendant l'envoi. Lire l'état d'un dépôt et abandonner SON dépôt ne demandent que le droit (exception nommée `DELETE /transfers/{id}` : n'efface que la partie reçue du compte appelant). **Supprimer** un fichier, **revenir à la version précédente** et **envoyer avec « Remplacer »** ne sont jamais couverts (actes `0x17`, `0x18` et le geste `replace`) : ils viennent avec l'onglet Fichiers.

## Application (code)
- `crates/hearth-proto/src/admin_act.rs::ActKind::FileWrite, ::AdminAct::file_deposit, ::covered_by_elevation, ::NOT_AN_ACT`
- `crates/hearth-agent/src/entrypoint/http/reauth.rs::Parsed`
- `crates/hearth-link/src/domain/act.rs::build`

## Vérification
- `crates/hearth-proto/src/admin_act.rs::tests`
- `crates/hearth-agent/tests/admin_reauth.rs` (matrice des actes, `Spec::Deposit`)
- `crates/hearth-agent/tests/service_transfers_https.rs::opening_a_deposit_needs_the_right_to_manage_services`

## Cas limites
- Un client qui ne signe pas le nom exact ni la taille exacte voit sa preuve refusée.
- Le droit retiré pendant un envoi ferme le socket (`MISSING_RIGHT`), la partie reste reprenable par un compte qui l'a.

## Règles liées
- BR-FILES-019
- BR-GRANT-003
- BR-TRUST-043

## Historique
- 2026-11-15 : création (HRT-95, côté agent, zone d'attente).
