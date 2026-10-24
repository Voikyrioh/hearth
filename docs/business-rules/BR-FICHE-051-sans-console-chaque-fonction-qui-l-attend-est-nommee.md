---
id: BR-FICHE-051
domaine: FICHE
titre: Sans console, tout ce qui est faisable est livré et chaque fonction qui attend la console est nommée, présente, inactive, avec son texte
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-051), HRT-109
maj: 2026-10-24
---

# BR-FICHE-051

## Règle
Sous « Ce qui attend la console » (repliée), une ligne par fonction que le jeu offre par sa console et que Hearth n'offre pas encore : le bouton n'est pas caché, il est présent et inactif, avec « Cette fonction attend la console de Hearth, qui n'est pas encore livrée. » suivi du contournement. Trois fonctions valent pour tout jeu et sont écrites dans le client : appliquer un réglage sans redémarrer, voir qui est connecté, écrire à tous les joueurs. Celles qui sont propres à un jeu (bannir, régler l'heure, enregistrer le monde à la demande) ne peuvent pas être écrites dans un client sans règle de jeu : elles viendront de la fiche.

## Application (code)
- `apps/desktop/src/components/molecules/ConsoleWaitSection.vue`

## Vérification
- `apps/desktop/src/components/organisms/gameSettingsTab.test.ts::le fichier brut, la console et les autres réglages`

## Cas limites
- Une fonction que Hearth n'a pas essayée sur la vraie image n'est pas proposée non plus (BR-FICHE-057) : elle a sa phrase de remplacement.

## Règles liées
BR-FICHE-057

## Historique
- 2026-10-24 — création (HRT-109).
