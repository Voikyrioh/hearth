---
id: FIX-01M7GSNNTE0GP22STY1VRJ56YG
titre: Un pack Fabric 1.20.1 installé avec la version « LATEST » faisait planter le conteneur en boucle (HRT-225)
date_découverte: 2026-11-16
date_correction: 2026-11-16
---

# FIX-01M7GSNNTE0GP22STY1VRJ56YG : Version LATEST appliquée à un pack qui demande une version précise (HRT-225)

Sévérité : majeur (conteneur en boucle de plantage).

## Symptôme
Installation d'un serveur Minecraft Fabric avec un pack serveur Fabric 1.20.1 : la version proposée est « LATEST » (26.3, Java 25), le pack ne démarre pas.

## Reproduction
`tests/minecraft_install_https.rs::a_fabric_1_20_1_pack_installs_with_the_game_version_it_says_not_latest` (sans le correctif : `VERSION=LATEST`).

## Cause root
L'agent jugeait le genre du pack mais ne lisait ni sa version ni son moteur ; la réponse « version » gardait sa valeur proposée.

## Impacté
`domain/mods/pack.rs`, `application/fiche_install.rs`.

## Workaround
Saisir la version à la main.

## Correction
`domain/mods/game.rs` lit le jeu et le moteur dans le pack ; `apply_pack_game` applique la version lue à la place de LATEST, ou refuse (version illisible, version contradictoire, moteur contradictoire). Voir BR-MOD-019. Limite : une modification faite après l'installation n'est pas encore contrôlée.

## Références
- HRT-225 · BR-MOD-019
