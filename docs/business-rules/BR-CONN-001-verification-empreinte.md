---
id: BR-CONN-001
domaine: CONN
titre: L'empreinte du serveur est affichée en 8 groupes de 4 hexadécimaux et confirmée avant la première connexion
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-001), technique-socle §9 « Empreinte », ADR-0005
maj: 2026-10-04
---

# BR-CONN-001 — Vérification de l'empreinte à la première connexion

## Règle
L'empreinte du serveur doit être vérifiée et confirmée explicitement par l'utilisateur avant la première connexion. Elle est le SHA-256 du certificat de l'agent encodé en DER. Format affiché : les 16 premiers octets, en 8 groupes de 4 caractères hexadécimaux **majuscules** séparés par une espace (`A1B2 C3D4 E5F6 0718 293A 4B5C 6D7E 8F90`). La comparaison interne porte toujours sur les 32 octets, jamais sur l'affichage court.

La confirmation par l'utilisateur relève du client (`hearth-link`, interface) ; cette fiche couvre le calcul et le format, côté agent (commande `hearth-agent fingerprint`) comme côté client : le code est dans `hearth-proto` pour que `hearth-link` affiche exactement la même chose.

## Application (code)
- `crates/hearth-proto/src/fingerprint.rs::Fingerprint::of_certificate_der` (L36) — SHA-256 du certificat DER.
- `crates/hearth-proto/src/fingerprint.rs::Fingerprint::short` (L68) — affichage 8 × 4 majuscules (16 premiers octets) ; `Display` y renvoie.
- `crates/hearth-proto/src/fingerprint.rs` — `#[derive(PartialEq)]` sur les 32 octets.
- `crates/hearth-agent/src/app.rs::run` — sous-commande `fingerprint` : affiche la forme courte.

## Interface (coquille et vue)
- `apps/desktop/src/components/molecules/FingerprintBlock.vue` : l'empreinte en 8 groupes de 4 sur 2 lignes ; `components/organisms/AddServerWizard.vue` (2e temps « Vérifie l'identité du serveur », boutons « Refuser » / « Confirmer ») ; logique : `composables/useAddServer.ts` (`next`, `confirm`, `refuse`).
- Tests : `src/composables/useAddServer.test.ts`, `src/components/organisms/connect.test.ts`, `e2e/connect.spec.ts` (empreinte refusée puis acceptée).

## Vérification
- Tests : `hearth_proto::fingerprint::tests` (`short_form_is_eight_groups_of_four_uppercase_hex`, `short_form_uses_only_the_first_sixteen_bytes`, `equality_compares_all_thirty_two_bytes`, `hashes_the_der_with_sha256`, `parse_rejects_a_sign_in_front_of_a_byte`).
- Intégration : `crates/hearth-agent/tests/hello.rs::hello_is_served_over_tls13_with_the_pinned_certificate` — l'empreinte du certificat présenté en TLS égale celle de la commande `fingerprint`.
- À la main : `hearth-agent fingerprint --data-dir <dossier>`.

## Cas limites
- Deux certificats qui ne diffèrent que par les octets 17 à 32 du SHA-256 ont le même affichage court mais ne sont pas égaux.
- Aucun certificat encore généré : `fingerprint` en crée un (BR-INSTALL-004), puis l'affiche.

## Règles liées
- BR-INSTALL-004 (l'empreinte ne change jamais après la première installation).
- BR-CONN-002 (mémorisation et vérification côté client, `hearth-link`), BR-CONN-003 (empreinte changée), BR-CONN-011 (rien n'est envoyé avant confirmation).

## Historique
- 2026-10-04 — création (HRT-02, session 2026-10-04-hearth-creation).
- 2026-10-04 — `Fingerprint` déplacée dans `hearth-proto` ; l'analyse de la forme complète refuse tout caractère non hexadécimal (review Stephen round 1).
- 2026-10-05 : section Interface (HRT-10).
