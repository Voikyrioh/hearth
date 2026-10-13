---
id: BR-ADOPT-025
domaine: ADOPT
titre: Si le lien coupe après l'envoi, l'agent va au bout de l'acte et ne le rejoue jamais ; le client relit la liste
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-025), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-80
maj: 2026-10-13
---

# BR-ADOPT-025 — Si le lien coupe après l'envoi, l'agent va au bout de l'acte et ne le rejoue jamais ; le client relit la liste

## Règle
Les deux routes d'acte sont suivies par clé d'opération (`Endpoint::tracked`) : une requête qui porte une `Idempotency-Key` s'exécute dans une tâche détachée, va jusqu'au bout même si le client coupe, et son résultat est retenu ; la rejouer avec la même clé rend le premier résultat sans l'exécuter de nouveau (une seule entrée de journal). Rien dans l'agent ne rejoue un acte tout seul. Le message « Le résultat de cette action n'est pas connu. » et la relecture de la liste au retour du lien sont ceux du client (`BR-RESIL-009`, `BR-RESIL-010`).

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/operations.rs::track`
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS`

## Vérification
- `crates/hearth-agent/tests/service_adoption_https.rs::a_link_cut_after_the_request_leaves_the_act_done_once_and_a_replay_never_does_it_twice`

## Règles liées
- BR-ADOPT-013, BR-RESIL-009

## Historique
- 2026-10-13 — création (HRT-80, ADR-0043 : l'acte d'adopter et de relâcher, moitié agent et protocole).
