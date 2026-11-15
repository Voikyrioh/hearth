---
id: BR-ADOPT-022
domaine: ADOPT
titre: Un port ou un nom tenu par un conteneur non adopté est dit en nommant le conteneur, ou son image
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-022), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-79
maj: 2026-11-14
---

# BR-ADOPT-022 — Un port ou un nom tenu par un conteneur non adopté est dit en nommant le conteneur, ou son image

## Règle
Un port du serveur ou un nom de service tenu par un conteneur non adopté est dit en nommant le conteneur, à défaut son image, pour que l'administrateur sache où regarder : « Ce port est tenu par « {nom} », un conteneur créé hors de Hearth. » ; « Ce nom est tenu par « {nom} », un conteneur créé hors de Hearth. Choisissez un autre nom. » (la phrase dit quoi faire ensuite, constat Nora 1). Le texte nommé est le nom du conteneur ; sans nom, l'image (référence et version), à défaut son identifiant court ; borné à 64 caractères, sans caractères de contrôle (`conflict_label`). **Jamais** une variable, une commande, un dossier ni un état : la fonction ne reçoit que le nom et l'image. Sans nom connu (écouteur `docker-proxy` que le moteur ne liste pas), le message dit seulement « créé hors de Hearth ». Pour un port, un conteneur arrêté hors Hearth n'est pas nommé (il ne bloque pas) : le client lirait « service arrêté ». Pour un nom, arrêté ou non, il bloque et il est nommé.

Le registre répond à « qui tient ce port » et « qui porte ce nom » par `Holder` : `Service(nom)` pour un service publié (déjà dans la liste), `Unmanaged` sans charge utile (la suppression le lit aussi, BR-ADOPT-002). Le nom ou l'image d'un conflit vient de `conflict_label`, demandé à part. Un conteneur arrêté tient encore son port et son nom.

## Application (code)
- `crates/hearth-agent/src/domain/services/visibility.rs::{Holder, conflict_label}`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::{port_holder, name_claim}`

## Vérification
- `crates/hearth-agent/src/application/services.rs::tests::a_port_or_a_name_held_by_a_container_that_is_not_adopted_names_the_container`
- `crates/hearth-agent/src/domain/services/visibility.rs::tests::a_holder_that_is_not_published_carries_nothing_to_leak`
- `crates/hearth-agent/src/domain/services/visibility.rs::tests::a_conflict_names_the_container_or_else_its_image`

## Cas limites
- Un service en réseau hôte n'a pas de port listé : il n'est pas vu par `port_holder`.

## Règles liées
- BR-ADOPT-001, BR-ADOPT-002

## Historique
- 2026-10-13 — création (HRT-79, ADR-0043 : ce que l'agent publie, moitié agent, première partie).
- 2026-11-14 — amendée (HRT-214, décision de Voiky : nommer le conteneur ou son image, constats Nora 10 et 11).
