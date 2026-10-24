---
id: BR-FICHE-035
domaine: FICHE
titre: Un compte qui perd le droit entre l'ouverture du formulaire et le lancement est refusé, et garde sa saisie ; l'installation en cours n'est pas interrompue
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-035), HRT-101
maj: 2026-10-24
---

# BR-FICHE-035

## Règle
Le droit « gérer les services » est relu à CHAQUE requête par la couche d'accès (BR-GRANT-010) : l'installation est refusée (`403`, rien n'est créé) si le droit a été retiré depuis l'ouverture du formulaire ; une installation déjà acceptée n'est pas interrompue par un retrait du droit. Le message et la conservation de la saisie sont au client.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/auth.rs::allows`

## Vérification
- `crates/hearth-agent/tests/fiche_install_https.rs::an_account_without_the_right_creates_no_service`

## Cas limites
- La conservation de la saisie est au client (HRT-102).

## Règles liées
- BR-FICHE-012, BR-GRANT-010

## Historique
- 2026-10-24 — création (HRT-101).
