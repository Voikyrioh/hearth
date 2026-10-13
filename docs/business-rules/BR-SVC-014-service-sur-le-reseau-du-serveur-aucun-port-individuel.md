---
id: BR-SVC-014
domaine: SVC
titre: Un service sur le réseau du serveur n'a pas de port individuel : il le dit, rien n'est inventé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-014), HRT-62
maj: 2026-10-13
---

# BR-SVC-014 — Un service sur le réseau du serveur n'a pas de port individuel : il le dit, rien n'est inventé

## Règle
Les services qui utilisent directement le réseau du serveur n'exposent pas de port individuel : aucun port n'est inventé pour eux, et la liste ne dit jamais « Aucun port » (faux et rassurant : un tel service écoute sur toutes les interfaces du serveur). La cellule des ports dit « Utilise directement le réseau du serveur : aucun port individuel. » (texte de la spec, repris du détail du service), et le nom accessible de la ligne le répète.

Amendé par HRT-79 (ADR-0043, BR-ADOPT-022) : le récapitulatif des ports à venir ne liste que les services publiés ; un port tenu par un conteneur non adopté n'y est pas, il n'apparaît qu'à la saisie d'un déploiement, sans nom (`ServicesRegistry::port_holder`).

PÉRIMÈTRE DE CETTE FICHE (HRT-62, lecture seule) : seule cette part de la règle est livrée. Le récapitulatif des ports (une ligne par port, « Joignable depuis », « Actif » ou « Réservé, service arrêté », conflits, liste « Utilisent directement le réseau du serveur ») n'est pas dans ce ticket : il reste à faire et cette fiche sera complétée à ce moment.

## Application (code)
- `apps/desktop/src/components/molecules/PortChips.vue` (cas `hostNetwork`)
- `apps/desktop/src/services/format.ts::rowLabel`
- Le fait vient de l'agent : `crates/hearth-proto/src/api/services.rs::ServiceItem (host_network)`, relayé par `apps/desktop/src-tauri/src/services/dto.rs::ServiceDto`.
- HORS DOMAIN : règle d'affichage portée par l'interface.

## Vérification
- `apps/desktop/src/pages/Services.test.ts` : « service sur le réseau du serveur : le dit, jamais « Aucun port » ».
- `apps/desktop/src/components/organisms/serviceTable.test.ts` : « service sur le réseau du serveur : la cellule le dit ».
- `apps/desktop/src/services/services.test.ts` : « nom accessible ».

## Règles liées
- BR-SVC-002, BR-SVC-021

## Historique
- 2026-10-13 — création (HRT-62, section Services du client, lecture seule), part « réseau du serveur » seulement.
- 2026-10-13 — HRT-79 (ADR-0043) : le futur récapitulatif des ports ne listera que les services publiés.
