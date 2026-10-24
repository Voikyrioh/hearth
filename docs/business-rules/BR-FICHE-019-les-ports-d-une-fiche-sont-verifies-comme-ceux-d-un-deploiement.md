---
id: BR-FICHE-019
domaine: FICHE
titre: Le port de chaque port de la fiche est proposé par la fiche, choisi par l'utilisateur et vérifié comme en BR-DEPLOY-007 à 009
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-019), HRT-101
maj: 2026-10-24
---

# BR-FICHE-019

## Règle
Chaque port publié de la fiche prend le numéro choisi par l'utilisateur (sinon celui de la fiche) ; un port `optional` n'est publié que si l'utilisateur le demande, un port `never` (console distante) jamais. Un port sous 1024 ou en double est refusé par la traduction ; les ports choisis sont ensuite jugés par le même contrôle qu'un déploiement ordinaire (`PORT_IN_USE`, réservation d'un service arrêté à cocher).

Deux règles de port portées par le client (HRT-102) : (1) un port d'une autre variante que celle choisie ne part jamais et n'est jamais listé (`only_variants` ; vide ou absent : le champ est omis, `[]` est refusé au chargement, BR-FICHE-060) ; (2) en `same_number`, le jeu annonce son port : le port du service, celui du serveur et celui du jeu valent le même nombre, le récapitulatif dit « {port} vers {port} » et la consultation de l'agent reçoit le même port du service.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/settings_state.rs::chosen_host_port`
- `crates/hearth-agent/src/domain/catalog/translate.rs::translate`
- `crates/hearth-agent/src/application/deployments.rs::Deployments::check_ports_for`
- `apps/desktop/src/fiche/ports.ts::forVariant`
- `apps/desktop/src/fiche/ports.ts::containerPortOf`
- `apps/desktop/src/fiche/ports.ts::readableHostPorts`
- `apps/desktop/src/components/organisms/FicheSummaryDialog.vue`
- `apps/desktop/src/components/molecules/FichePortField.vue`

## Vérification
- `crates/hearth-agent/tests/fiche_install_https.rs::a_rich_install_writes_each_file_with_the_declared_owner_and_never_wider_than_0640`
- `apps/desktop/src/fiche/fiche.test.ts::les ports d'une fiche (BR-FICHE-019)`
- `apps/desktop/src/link/simulated-catalog.test.ts::ne publie que les ports de la variante, et donne au service le port du serveur quand le jeu annonce le sien`
- `apps/desktop/src/pages/InstallFromFiche.test.ts::le récapitulatif suit la variante et le mapping des ports (BR-FICHE-019, 060)`
- `apps/desktop/src/pages/InstallFromFiche.test.ts`

## Cas limites
- Le décalage d'un groupe de ports liés et les messages sont portés par le formulaire (HRT-102).

## Règles liées
- BR-DEPLOY-007, BR-DEPLOY-009

## Historique
- 2026-10-24 — création (HRT-101).
- 2026-10-24 — côté client (HRT-102).
