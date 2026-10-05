---
id: BR-RESIL-011
domaine: RESIL
titre: Aucune fenêtre bloquante pour une perte de lien ou une erreur ; notifications discrètes
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-011), HRT-09
maj: 2026-10-05
---

# BR-RESIL-011 — Aucune fenêtre bloquante pour une perte de lien ou une erreur ; notifications discrètes

## Règle
Une perte de lien, une erreur de reconnexion, l'issue d'une opération incertaine ou une erreur d'interface ne produisent jamais de modale : une notification discrète s'empile en bas à droite (3 visibles au plus, fermable, disparition seule après 6 s, annoncée aux lecteurs d'écran, jamais de vol de focus). Issues d'opération (BR-RESIL-010), précédées du nom du serveur concerné (« forge : Fait pendant la coupure. », « nas-salon : Non exécuté. Tu peux relancer. », « forge : Résultat inconnu. Vérifie l'état du serveur. ») ; l'issue reste consultable par `opId` (`link.outcomeOf`, bornée en nombre et en âge). Une erreur non rattrapée (rendu, gestionnaire d'événement, promesse rejetée) donne « Un problème est survenu dans l'interface. Il a été noté dans le journal. » et une ligne dans le journal du client ; seule une erreur de RENDU d'une page la remplace par un message avec « Réessayer » ; les erreurs de gestionnaire et les promesses rejetées ne remplacent rien ; la coquille (barre, navigation, en-tête, pastille, bandeau) n'est dans aucune frontière et reste toujours utilisable.

## Application (code)
- `apps/desktop/src/stores/toasts.ts` ; `apps/desktop/src/components/molecules/ToastStack.vue` ; `apps/desktop/src/stores/link.ts` (issues d'opération → notifications).
- `apps/desktop/src/errors/{report,install}.ts` ; `apps/desktop/src/components/molecules/ErrorBoundary.vue` ; commande Rust `log_frontend_error` (`apps/desktop/src-tauri/src/commands.rs`, bornes dans `domain.rs`).

## Vérification
- Tests : `molecules.test.ts::ToastStack`, `::ErrorBoundary`, `link-stores.test.ts::toasts store`, `::turns operation outcomes into discreet notifications`, `errors.test.ts`, `shell.test.ts::error containment`, `src-tauri/tests/domain.rs::frontend_errors_are_rate_limited_per_window`, `::a_frontend_text_cannot_forge_log_lines`, `::long_frontend_messages_are_cut_on_a_character_boundary`, `e2e/shell.spec.ts` (notifications).

## Cas limites
- Seule modale de l'application hors cette règle : `ConfirmDialog` pour une action destructrice voulue par l'utilisateur, et l'empreinte changée.
- Journal : une entrée = une ligne (retours à la ligne et caractères de contrôle neutralisés, `domain.rs::single_line`), message coupé à 2000 caractères, au plus 10 lignes par 10 s côté interface et 20 par minute côté Rust.

## Règles liées
- BR-RESIL-010, BR-RESIL-018.

## Historique
- 2026-10-05 — création (HRT-09, session 2026-10-04-hearth-creation). Portée par l'interface ; le calcul des états est dans `hearth-link` (ADR-0007).
