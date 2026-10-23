---
id: BR-FICHE-032
domaine: FICHE
titre: Chaque installation terminée est inscrite au journal d'activité avec la fiche, l'image, les ports, le nombre de réponses et la cause typée, sans aucune valeur de réponse
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-032), HRT-101
maj: 2026-10-24
---

# BR-FICHE-032 — Journal d'une installation

## Règle
Une installation depuis une fiche, réussie ou non, est une entrée `service.deploy` dont la cible dit : le service, la fiche et sa version, l'image (et son empreinte quand elle est épinglée), les ports du serveur, le NOMBRE de réponses, le processeur et la mémoire choisis et le titre de chaque contrat accepté (`Target::GameInstall`). Le type n'a pas de champ pour une valeur : aucune réponse, aucun nom de monde, aucun mot de passe, aucun secret tiré, aucun chemin. Les causes typées s'ajoutent à celles de BR-DEPLOY-025 : « machine trop petite », « espace insuffisant pour l'installation », « fiche inconnue », « fiche changée depuis l'affichage », « contrat non accepté », « fiche refusée ». Une fiche écartée au chargement de l'agent est inscrite une fois au démarrage (`catalog.fiche.refused`, auteur : le système). Écart assumé avec la spec : l'action reste `service.deploy` (« Déploiement d'un service »), parce qu'une installation par fiche est un déploiement (BR-FICHE-013) ; la cible dit la fiche.

## Application (code)
- `crates/hearth-agent/src/domain/audit/event.rs::{Target::GameInstall, Reason}`
- `crates/hearth-agent/src/application/deployments.rs::Deployments::record`
- `crates/hearth-agent/src/entrypoint/http/auth.rs::failure_of`

## Vérification
- `crates/hearth-agent/src/domain/audit/event.rs::a_game_install_entry_carries_no_answer_value`
- `crates/hearth-agent/tests/fiche_install_https.rs::a_fiche_install_creates_a_service_with_the_fiche_digest_the_pinned_image_and_a_0600_root_config_file`
- `crates/hearth-agent/tests/fiche_install_secrets.rs::no_answer_value_nor_generated_secret_reaches_the_database_the_journal_the_traces_or_a_response`

## Règles liées
- BR-FICHE-013, BR-DEPLOY-025, BR-AUDIT-005

## Historique
- 2026-10-24 — création (HRT-101, étapes S7 et S9).
