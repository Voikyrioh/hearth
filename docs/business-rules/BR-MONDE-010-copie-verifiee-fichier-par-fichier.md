---
id: BR-MONDE-010
domaine: MONDE
titre: La copie est relue et comparée par empreinte, fichier par fichier ; une copie incomplète ne crée rien
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-reprendre-un-monde-existant.md (BR-MONDE-010), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 9.9), HRT-105
maj: 2026-11-14
---

# BR-MONDE-010 — La copie est relue et comparée par empreinte, fichier par fichier ; une copie incomplète ne crée rien

## Règle
Chaque fichier est lu en flux avec un SHA-256 calculé à la lecture de la source ; la copie écrite est ensuite relue et comparée (taille et empreinte). Un fichier absent ou différent (hors fichiers notés changés) fait échouer la reprise : le dossier d'import est retiré, aucun service, aucune description, aucun conteneur n'est créé.

## Application (code)
- `crates/hearth-agent/src/application/world_import/copy.rs::copy_world`
- `crates/hearth-agent/src/application/deployments.rs::Deployments::import_world`

## Vérification
- `crates/hearth-agent/src/application/world_import/copy.rs::tests::a_copy_that_differs_from_the_source_is_incomplete`
- `crates/hearth-agent/tests/world_import_https.rs::a_copy_that_differs_from_the_source_creates_nothing_and_removes_only_its_import`

## Cas limites
- La relecture lit la copie telle que le système la rend (cache de pages) ; le fichier est rendu durable (`fsync`) avant.

## Règles liées
- BR-MONDE-009, BR-MONDE-004

## Historique
- 2026-11-14 — création (HRT-105, côté agent).
