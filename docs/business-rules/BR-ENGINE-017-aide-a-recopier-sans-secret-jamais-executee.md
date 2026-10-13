---
id: BR-ENGINE-017
domaine: ENGINE
titre: L'aide est du texte à recopier, adapté à la distribution, sans secret ni donnée de compte, jamais exécuté par Hearth
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-017), HRT-62
maj: 2026-10-13
---

# BR-ENGINE-017 — L'aide est du texte à recopier, adapté à la distribution, sans secret ni donnée de compte, jamais exécuté par Hearth

## Règle
Les commandes et les textes de configuration d'aide affichés sont du texte à recopier, adapté à la distribution du serveur. Ils ne contiennent aucun secret ni aucune donnée propre à un compte Hearth, et Hearth ne les exécute jamais.

Garde de sécurité (revue de la PR 79) : la seule valeur venue de l'agent qui entre dans un bloc à recopier est le nom du compte sous lequel l'agent tourne. Elle n'y entre que si c'est un nom de compte POSIX strict (`^[a-z_][a-z0-9_-]{0,31}$`, le `$` final des comptes de machine n'est pas toléré). Sinon le bloc porte l'emplacement littéral `<compte-de-l-agent>` et une note le dit ; la valeur reçue n'y figure jamais. La famille de distribution se choisit dans une liste fermée (`Map`) d'après `os_id` et `os_like` : aucune de ces valeurs n'entre dans une commande.

## Application (code)
- `apps/desktop/src/services/help.ts::helpFor`, `::isPosixAccount`, `::familyOf`
- `apps/desktop/src/components/molecules/CommandBlock.vue` : bloc à recopier, copie seule, aucune exécution.
- Textes : `apps/desktop/src/i18n/fr.ts` (`engine.help.*`).
- HORS DOMAIN : règle portée par l'interface (c'est elle qui affiche). La validation du nom côté agent reste à faire (ticket de suivi).

## Vérification
- `apps/desktop/src/services/services.test.ts` : « le nom du compte venu de l'agent n'entre dans une commande qu'après validation (I2) » (espaces, `;`, `$()`, guillemets inversés, retour à la ligne, Unicode, longueur, `$` final) et « la distribution se choisit dans une liste fermée ».
- `apps/desktop/src/pages/Services.test.ts` : accès refusé avec un nom de compte hostile.

## Cas limites
- Distribution inconnue, ou pas de systemd pour démarrer : renvoi à la documentation, aucune commande.

## Règles liées
- BR-ENGINE-005, BR-ENGINE-006, BR-ENGINE-008, BR-ENGINE-016

## Historique
- 2026-10-13 — création (HRT-62, section Services du client, lecture seule).
- 2026-10-13 — revue de la PR 79 (I2) : validation stricte du nom de compte, emplacement littéral sinon, liste fermée pour la distribution.
