---
id: ADR-0058
titre: Modifier un service se fait par recréation : image récupérée avant tout arrêt, ancien conteneur renommé (jamais supprimé) jusqu'au succès, nouveau jugé démarré après 10 secondes, retour arrière automatique, jamais un service qui repart seul
type: architecture
statut: acceptée
date: 2026-11-14
portee: projet
remplace: — (complète l'ADR-0039 et l'ADR-0044 pour la modification, l'ADR-0057 pour la suppression de l'ancien)
liens: [ADR-0039, ADR-0044, ADR-0057, HRT-87, BR-EDIT-002 à BR-EDIT-037]
---

# ADR-0058 : Modifier un service par recréation avec retour arrière

## Contexte

Le moteur de conteneurs ne sait pas changer l'image, les ports ou les montages d'un conteneur existant : il faut en créer un autre. Le service garde son nom, son identité et ses dossiers. Le geste ne doit jamais laisser un service en pièces ni le perdre.

## Décision

1. **Recréation.** Le nouveau conteneur est créé depuis la description scellée relue de la base (révision candidate écrite avant la création) ; les valeurs non touchées viennent de la révision précédente, jamais du client (BR-EDIT-006).
2. **Image d'abord.** `pull` se fait service en marche, avant tout arrêt : un échec réseau ne coûte aucun arrêt (BR-EDIT-011, 021).
3. **L'ancien est renommé, pas supprimé.** `ContainerEngine::rename` met l'ancien de côté ; c'est le point de non-retour de la jugeote d'échec ; l'ancien n'est retiré (sans volumes ni force, ADR-0057) qu'après la surveillance réussie (BR-EDIT-018).
4. **Succès = 10 secondes en marche** (BR-EDIT-019).
5. **Retour arrière sans demander** : nouveau retiré, révision candidate abandonnée (`ServiceSpecs::abandon`), ancien remis sous son nom et relancé s'il tournait (BR-EDIT-020). Si le retour échoue : opération `interrupted` (`step_failed`), l'ancien est gardé, aucun service ne repart seul (BR-EDIT-022). Après un redémarrage de l'agent, les décisions « Revenir » et « Garder » sont exécutées par `DecisionEffects`.
6. **Acte jamais couvert par l'élévation** (`ServiceUpdate` `0x13`) ; un service adopté n'est jamais modifié ; un service de fiche n'accepte que l'adresse des joueurs tant que la traduction des réponses n'existe pas (BR-EDIT-002, 017, 037).
7. **Journal sans nom ni valeur** : catégories changées et nombres seulement (BR-EDIT-033).

## Alternatives rejetées

- **Supprimer l'ancien avant de créer le nouveau** : aucun retour possible.
- **Retour arrière en mémoire seulement après redémarrage** : retenu pour les lignes de retour (24 h en mémoire) ; un redémarrage laisse l'opération `needs_attention`, jamais rejouée.
