# Bugs — Hearth

Fiches d'anomalies découvertes et corrigées : marquage `// FIX:{ULID}` dans le code, fiche explicative `FIX-{ULID}.md` documentant symptôme, cause root, workaround, correction.

Aucune fiche pour l'instant. À créer lors de bugfix conformément à `skills/bugfix/SKILL.md` et `docs/code-rules.md`.

Format fiche :

```markdown
---
id: FIX-{ULID}
titre: Titre court symptôme
date_découverte: 2026-10-04
date_correction: 2026-10-05
---

# FIX-{ULID} — {Titre}

## Symptôme
Comportement incorrect observé.

## Cause root
Explication technique, référence code.

## Impacté
- Versions < X
- Scénario : …

## Workaround
Si aucune correction immédiate.

## Correction
Code changé, commit hash, stratégie (patch, refactoring).

## Références
- Ticket : …
- ADR / BR : …
```

Pointeur code : `// FIX:{ULID}` au-dessus de la ligne corrigée.
