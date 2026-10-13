---
id: BR-SVC-021
domaine: SVC
titre: Tout ce qui vient d'un conteneur est nettoyé et plafonné avant le fil
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-021), revue de la PR 77 (HRT-61)
maj: 2026-10-13
---

# BR-SVC-021 — Tout ce qui vient d'un conteneur est nettoyé et plafonné avant le fil

## Règle
Un nom, une image ou un chemin allant jusqu'à 255 caractères ne casse jamais la mise en page (la coupure avec « … » et le survol sont portés par l'interface). Côté agent, pour que ni un conteneur hostile ni un conteneur étrange ne gonfle les instantanés de tous les abonnés ni ne glisse un retour à la ligne chez le client : tout texte venu d'un conteneur ou du moteur est débarrassé des caractères de contrôle puis coupé (« … » à la fin) à une longueur plafonnée (nom 255, référence d'image 255, version d'image 128, chemin de montage 1024, projet Compose 255, nom et version du moteur 64) ; un service garde au plus 128 ports publiés et 64 montages (les premiers, dans l'ordre trié). Les bornes sont des constantes du domaine.

## Application (code)
- `crates/hearth-agent/src/domain/services/limits.rs::bounded`
- `crates/hearth-agent/src/domain/services/limits.rs::MAX_PORTS`
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::observation_from`

## Vérification
- `crates/hearth-agent/src/domain/services/limits.rs::tests::a_long_text_is_cut_to_the_limit_with_a_mark`
- `crates/hearth-agent/src/domain/services/limits.rs::tests::control_characters_are_removed`
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::tests::everything_a_container_says_is_cleaned_and_bounded`

## Cas limites
- Au-delà de 128 ports ou 64 montages, les suivants sont omis sans compteur : le fil n'a pas de champ « et N autres » (à ajouter si l'interface en a besoin ; ce serait un ajout facultatif du fil).
- Les plages de ports contiguës ne sont pas regroupées.

## Historique
- 2026-10-13 — création (revue de la PR 77, HRT-61).
