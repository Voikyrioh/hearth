---
id: BR-FICHE-026
domaine: FICHE
titre: « Installer » exige l'acte confirmé du socle (mot de passe à chaque fois) et, s'il y a un contrat, son acceptation par un geste
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-026), HRT-101
maj: 2026-10-24
---

# BR-FICHE-026

## Règle
L'acte `ServiceDeploy` est confirmé comme pour toute image (preuve de clé du poste, mot de passe à chaque fois, jamais couvert par l'élévation). Quand la fiche a un contrat, l'agent refuse l'installation sans son acceptation (`AGREEMENT_REQUIRED`). La fenêtre d'acceptation et l'activation du bouton sont au client (HRT-102, HRT-129).

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/reauth.rs::Parsed::read`
- `crates/hearth-agent/src/domain/catalog/translate.rs::translate`
- `apps/desktop/src/components/organisms/FicheSummaryDialog.vue`
- `apps/desktop/src/components/organisms/AgreementDialog.vue`
- `apps/desktop/src/fiche/attention.ts`

## Vérification
- `crates/hearth-agent/tests/fiche_install_https.rs::an_install_without_the_agreement_is_refused_even_from_a_modified_client_and_creates_nothing`
- `apps/desktop/src/pages/InstallFromFiche.test.ts::le contrat de licence (BR-FICHE-026, 058)`

## Cas limites
- Le bouton et la fenêtre sont au client.

## Règles liées
- BR-FICHE-058, BR-DEPLOY-015

## Historique
- 2026-10-24 — création (HRT-101).
- 2026-10-24 — côté client (HRT-102).
