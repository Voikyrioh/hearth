---
id: BR-OPS-008
domaine: OPS
titre: La reprise au démarrage compte les conteneurs du service dans le vrai moteur et ne présume jamais un état cohérent
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0 bis et 7), contexts/hearth/tickets/hrt/HRT-144.md
maj: 2026-10-14
---

# BR-OPS-008 — La reprise au démarrage compte les conteneurs du service dans le vrai moteur et ne présume jamais un état cohérent

## Règle
Pour une opération non terminée, l'agent demande au moteur (lecture seule) combien de conteneurs portent la clé du service : `c_` par l'inspection de l'identifiant, `h_` par l'étiquette `io.hearth.service-id` dans la liste ; `u_` n'est pas observé (illisible). Un état n'est dit COHÉRENT que si tous les gestes destructifs du plan ont leur `done` ET que le moteur montre exactement ce que la fin du plan laisse (un conteneur, ou aucun pour une suppression). Plusieurs conteneurs : ambigu (« à vérifier »). Moteur muet, liste incomplète (un conteneur non inspectable), plan à moitié exécuté ou état qui ne correspond pas : illisible (« à vérifier »). Aucune écriture dans le moteur, jamais.

## Application (code)
- `crates/hearth-agent/src/domain/ops/observe.rs::observed_from_presence`
- `crates/hearth-agent/src/infrastructure/ops_observer.rs::EngineObserver`
- `crates/hearth-agent/src/app.rs::service_foundations_observed`

## Vérification
- `crates/hearth-agent/src/domain/ops/observe.rs::tests`
- `crates/hearth-agent/tests/service_ops_observer.rs`

## Cas limites
- Retirer un conteneur « créé jamais démarré » par une opération interrompue n'est pas livré : le port du moteur n'a aucune méthode qui écrit (HRT-68 ajoute `create` et `remove`). Seuls les objets de fichiers nommés au journal sont retirés.
- Les services supervisés (`u_`) ne sont pas observés ici : leurs opérations arrivent avec leur lot.

## Règles liées
- BR-EDIT-031, BR-LIFE-013, BR-EDIT-036

## Historique
- 2026-10-14 — création (HRT-144 : le socle des opérations branché sur des routes et sur le flux).
