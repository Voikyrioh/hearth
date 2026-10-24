---
id: BR-FICHE-004
domaine: FICHE
titre: Le détail d'une fiche dit ce qu'elle impose, avec l'empreinte de la fiche et la preuve de chaque chiffre de ressource, sans aucun secret
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-004), HRT-101
maj: 2026-10-24
---

# BR-FICHE-004 — Le détail d'une fiche

## Règle
`GET /catalog/{id}` rend : l'empreinte de la fiche (SHA-256 hexadécimal, celle que l'installation doit reprendre), l'image exacte avec sa version (et son empreinte quand elle est épinglée), le propriétaire numérique, les variantes, chaque port avec son protocole, son rôle et sa publication, chaque dossier géré avec son rôle et son chemin DANS le service (jamais un chemin du serveur), les contrats à accepter, les questions d'installation (réglages essentiels non imposés, avec leur genre, leurs bornes, leurs choix, leur jeu de caractères, la mention de figé et les variantes qu'elles concernent ; sans valeur proposée pour un secret) et le processeur, la mémoire et le disque avec, pour chacun, d'où vient le chiffre (mesuré, mesuré au repos, éditeur avec son adresse, non mesuré). Aucun chiffre n'est inventé : sans preuve, il est absent.

## Application (code)
- `crates/hearth-agent/src/application/catalog.rs::{CatalogService::detail, detail_of}`
- `crates/hearth-proto/src/api/catalog.rs::FicheDetail`
- `apps/desktop/src/components/organisms/FicheSheet.vue`
- `apps/desktop/src/pages/FicheDetail.vue`

## Vérification
- `crates/hearth-agent/src/application/catalog.rs::the_detail_shows_what_the_fiche_imposes_with_the_proof_of_each_figure_and_no_secret`
- `crates/hearth-agent/tests/catalog_https.rs::a_published_agent_lists_no_preview_fiche_and_a_trial_agent_lists_terraria_with_its_digest`
- `apps/desktop/src/pages/FicheDetail.test.ts`

## Cas limites
- Depuis HRT-102, le détail rend aussi le premier démarrage (durée annoncée, téléchargements avec leur preuve, repères d'étape), la façon d'arrêter le jeu et les réglages que la fiche impose avec leur raison ; la version du jeu et les mods ne sont pas encore rendus (HRT-103).
- Le client montre l'empreinte de la fiche, l'image, les ports, les dossiers (chemin dans le service), les ressources avec leur preuve, les téléchargements, l'arrêt du jeu, les questions et ce que la fiche impose ; aucune ligne « compte exigé » n'existe.

## Règles liées
- BR-FICHE-001, BR-FICHE-008, BR-FICHE-058

## Historique
- 2026-10-24 — création (HRT-101, étape S8).
- 2026-10-24 — côté client (HRT-102).
