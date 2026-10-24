---
id: BR-FICHE-024
domaine: FICHE
titre: Les dossiers d'une fiche sont tous gérés par Hearth ; le fichier produit des réponses est 0640 au propriétaire de la fiche, 0600 root pour une image root
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-024), HRT-101
maj: 2026-10-24
---

# BR-FICHE-024 — Dossiers et fichier produit

## Règle
Chaque dossier de la fiche est un dossier géré (nom aplati du point de montage, sous le dossier du service dans la racine de données) ; c'est le DOSSIER qui est monté dans le service, jamais un fichier seul. Le fichier de configuration produit à partir des réponses est écrit dans le dossier géré avant la création du conteneur, de droits `0640` et du propriétaire (nombres) déclaré par la fiche ; pour une image qui tourne en root (propriétaire uid 0) : `0600` et root. Le droit déclaré par la fiche (jusqu'à `0644`) est abaissé : le fichier peut porter un secret.

## Application (code)
- `crates/hearth-agent/src/domain/services/from_fiche.rs::{produced_file_perms, body_and_secrets_from}`

## Vérification
- `crates/hearth-agent/src/domain/services/from_fiche.rs::a_root_image_writes_0600_root_and_others_0640_with_the_declared_owner`
- `crates/hearth-agent/src/domain/services/from_fiche.rs::terraria_files_are_produced_root_0600_in_the_managed_config_folder`

## Cas limites
- Le fichier est volontairement en clair dans le dossier géré : il est exclu du balayage sentinelle (vérifié à part).

## Règles liées
- BR-FICHE-007, BR-DEPLOY-010

## Historique
- 2026-10-24 — création (HRT-101, étape S3).
