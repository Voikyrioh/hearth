---
id: BR-MOD-023
domaine: MOD
titre: Un jar déposé s'ajoute, remplace un mod installé ou est refusé, avant tout geste sur le serveur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft.md
maj: 2026-11-18
---

# BR-MOD-023 : Un jar déposé s'ajoute, remplace ou est refusé, avant tout geste

## Règle
`POST /services/{key}/mods/check` (droit « gérer les services », lecture seule) juge 1 à 8 dépôts de la zone d'attente du compte, chacun une seule fois. Pour chaque dépôt, dans cet ordre : archive refusée par le lecteur d'archives hostiles (`MOD_ARCHIVE_REFUSED`), fichier trop gros (`MOD_TOO_LARGE`), aucune fiche d'identité lisible (`MOD_UNRECOGNIZED`), moteur refusé (`MOD_INCOMPATIBLE_LOADER`, avec le moteur du mod et celui du serveur), version de Minecraft refusée (`MOD_INCOMPATIBLE_MINECRAFT`, avec la version du serveur), deux dépôts du même identifiant (`MOD_DUPLICATE_IN_BATCH`, les deux refusés), identifiant d'un mod du pack Modrinth (`MOD_IN_MODRINTH_PACK`), même version qu'un mod installé (`MOD_SAME_VERSION`), nom de pose déjà pris par un autre fichier, actif ou `.disabled` (`MOD_NAME_TAKEN`). Sinon : `add`, ou `replace` si un mod installé (actif ou désactivé, jamais « Mods retirés ») a le même identifiant ; l'ancien partira dans « Mods retirés » (HRT-258). Le nom de pose est `{identifiant}-{version}.jar`, choisi par Hearth, jamais le nom envoyé. Seuls les mods activés comptent pour les dépendances et les doublons ; les mods remplacés ne comptent plus. Un dépôt refusé est retiré de la zone d'attente aussitôt (BR-MOD-016) ; un dépôt accepté y reste. Un identifiant de dépôt inconnu (ou d'un autre compte) arrête tout le contrôle : `DEPOSIT_UNKNOWN`, rien n'est jugé ni retiré. Lot vide, de plus de 8 dépôts ou avec un identifiant répété : `MODS_BATCH_INVALID`. Moteur sans mods : `MODS_NOT_APPLICABLE`. Rien n'est écrit dans le service. Le client n'est jamais cru : HRT-258 rejuge tout à l'acceptation.

## Application (code)
- Client (HRT-262) : `apps/desktop/src/services/modsDeposit.ts` (phrases), `ModsCheckList.vue` (case de remplacement décochée d'office)
- `crates/hearth-agent/src/domain/mods/deposit.rs::{judge_deposits, Refusal::code}`
- `crates/hearth-agent/src/application/mods.rs::ModsService::check`, `entrypoint/http/mods.rs::check`

## Vérification
- `domain::mods::deposit::tests::*`
- `tests/mods_list.rs::a_check_*`

## Cas limites
- Version du serveur inconnue ou plage non comprise : « non vérifié », jamais un refus.
- Dépendance absente : `to_complete`, n'empêche pas d'ajouter ; un voisin du lot la satisfait.

## Règles liées
- BR-MOD-016, BR-MOD-018, BR-MOD-021, BR-MOD-022, BR-MOD-024, BR-MOD-031

## Historique
- 2026-11-18 : création (HRT-257).
