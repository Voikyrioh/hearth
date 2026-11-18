---
id: BR-MOD-025
domaine: MOD
titre: Un lot de mods s'applique en une seule opération du service, jamais sans mot de passe
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft.md
maj: 2026-11-18
---

# BR-MOD-025 : Un lot s'applique en une seule opération

## Règle
`POST /services/{key}/mods/apply` juge le lot SANS rien arrêter ni écrire (bornes : 1 à 64 changements dont 8 ajouts au plus, noms de l'alphabet sûr, aucun fichier deux fois ; dépôts rejugés entièrement, empreinte relue ; fichiers présents dans le bon état ; noms d'arrivée libres ; identifiant déjà actif ; espace pour la sauvegarde) puis accepte UNE opération `mods` (verrou exclusif du service ET place des opérations lourdes) : `stop` (si le service tournait), `copy` et `verify` (la sauvegarde « Avant mods »), `place`, `start` et `watch` (si le service tournait). Un service arrêté le reste. Ordre dans `place` : retraits (actifs et désactivés), désactivations, remplacements, réactivations, remises, ajouts (ce qui libère un identifiant passe avant ce qui l'occupe). Le client n'est jamais cru : tout est rejugé. Le geste est l'acte signé `ModsApply` (`0x20`), JAMAIS couvert par l'élévation : le mot de passe est toujours demandé. Une autre opération sur le service : `OPERATION_IN_PROGRESS` ; une autre opération lourde sur le serveur : `HEAVY_IO_BUSY`. Le journal d'activité (`service.mods`) écrit des nombres et des noms de fichiers de l'alphabet sûr, jamais un chemin ; l'origine `hearth` des fichiers posés encore présents s'écrit dans la MÊME transaction que la clôture (BR-MOD-033).

## Application (code)
- `crates/hearth-agent/src/domain/mods/apply.rs::{validate, plan}`, `domain/ops/mods.rs::op_plan`
- `crates/hearth-agent/src/application/mods_apply.rs::ModsService::apply`
- `crates/hearth-proto/src/admin_act.rs::AdminAct::mods_apply` (acte `0x20`), `domain/trust/admin_act.rs::covered_by_elevation`

## Vérification
- `tests/mods_apply_https.rs` (chemin nominal, service arrêté, chaque refus synchrone ne touche à rien, acte non signé, lecteur sans droit, `OPERATION_IN_PROGRESS`, `HEAVY_IO_BUSY`)
- `tests/admin_reauth.rs` (matrice des actes)

## Règles liées
- BR-MOD-024, BR-MOD-026, BR-MOD-027, BR-MOD-031, BR-BACKUP-010

## Historique
- 2026-11-18 : création (HRT-258).
