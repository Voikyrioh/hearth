---
id: BR-SVC-024
domaine: SVC
titre: Lire la liste des services n'est pas journalisé
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-024), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-024 — Lire la liste des services n'est pas journalisé

## Règle
Lire la liste, le détail et le récapitulatif des ports sont des consultations, non journalisées. `GET /services`, `POST /engine/recheck` et le sujet `services` n'ont aucune action de journal. Exception (HRT-63) : la révélation des valeurs des variables, `GET /services/{key}/env`, est consignée (BR-SVC-009), réussite et refus ; ce n'est pas une consultation ordinaire.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS (`audit: None`, sauf `/services/{key}/env`)`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::reading_the_engine_and_the_services_writes_nothing_to_the_journal`

## Règles liées
- BR-ENGINE-020

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
