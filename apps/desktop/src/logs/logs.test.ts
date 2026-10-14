import { describe, expect, it } from "vitest";
import { type Entry, LogBuffer, lineEntry, MAX_CHARS, MAX_LINES } from "./buffer";
import { findMatches, segmentsOf } from "./search";
import { displayText, FOLD_CHARS, foldedText, MAX_LINE_CHARS, stripTerminal } from "./text";
import { clockOf, fullTimeOf } from "./time";
import { endNotice, exitWords, gapText, waitingText } from "./view";
import {
  atBottom,
  estimateHeight,
  indexAt,
  layout,
  linesFor,
  type Metrics,
  visibleRange,
} from "./windowing";

const ESC = String.fromCharCode(27);
const BEL = String.fromCharCode(7);
const line = (text: string, t: number | null = 1): ReturnType<typeof lineEntry> =>
  lineEntry({ t, source: "out", text, cut: null });

describe("texte d'une ligne : rien n'est interprété (BR-SLOG-013, BR-SLOG-019)", () => {
  it("retire les séquences de terminal, titre de fenêtre et lien compris", () => {
    expect(stripTerminal(`${ESC}[31mrouge${ESC}[0m ok`)).toBe("rouge ok");
    expect(stripTerminal(`${ESC}]0;titre${BEL}apres`)).toBe("apres");
    expect(stripTerminal(`a${ESC}]8;;https://x.invalid${ESC}\\clic${ESC}]8;;${ESC}\\b`)).toBe(
      "aclicb",
    );
    expect(stripTerminal(`${ESC}[2J${ESC}[Hvide`)).toBe("vide");
    expect(stripTerminal(`${String.fromCharCode(0x9b)}31mrouge`)).toBe("rouge");
  });

  it("rend visibles les contrôles et le bidirectionnel, garde les tabulations en espaces", () => {
    expect(displayText("a\u0000b\u0007c")).toBe("a·b·c");
    expect(displayText("x‮errof‬")).toBe("x·errof·");
    expect(displayText("a\tb")).toBe("a    b");
    expect(displayText(null)).toBe("");
  });

  it("n'interprète jamais du HTML ni un lien : le texte reste du texte", () => {
    const hostile = "<img src=x onerror=alert(1)> https://exemple.invalid";
    expect(displayText(hostile)).toBe(hostile);
  });

  it("borne une ligne énorme sans couper une paire de substitution", () => {
    const huge = `${"a".repeat(MAX_LINE_CHARS - 1)}😀😀😀`;
    const shown = displayText(huge);
    expect(Array.from(shown).length).toBe(MAX_LINE_CHARS);
    expect(displayText("a".repeat(64 * 1024)).length).toBe(MAX_LINE_CHARS);
    expect(foldedText("b".repeat(FOLD_CHARS + 50)).length).toBe(FOLD_CHARS);
    expect(foldedText("court")).toBe("court");
  });

  it("une séquence jamais terminée ne fige pas et ne laisse rien derrière elle", () => {
    const started = Date.now();
    stripTerminal(`${ESC}]${"x".repeat(100_000)}`);
    stripTerminal(`${ESC}[${"1;".repeat(50_000)}`);
    expect(Date.now() - started).toBeLessThan(2000);
  });
});

describe("tampon : mémoire bornée, retrait annoncé (BR-SLOG-006)", () => {
  it("garde 5 000 lignes au plus et compte celles qui partent", () => {
    const buffer = new LogBuffer();
    const batch = Array.from({ length: MAX_LINES + 700 }, (_, i) => line(`l${i}`, i));
    const dropped = buffer.append(batch);
    expect(dropped).toBe(700);
    expect(buffer.removed).toBe(700);
    expect(buffer.entries).toHaveLength(MAX_LINES);
    expect(buffer.entries[0]?.kind === "line" && buffer.entries[0].text).toBe("l700");
  });

  it("borne aussi le nombre de caractères gardés", () => {
    const buffer = new LogBuffer();
    const big = "z".repeat(MAX_LINE_CHARS);
    buffer.append(Array.from({ length: MAX_CHARS / MAX_LINE_CHARS + 50 }, () => line(big)));
    expect(buffer.entries.length).toBeLessThanOrEqual(MAX_CHARS / MAX_LINE_CHARS);
    expect(buffer.removed).toBeGreaterThan(0);
  });

  it("regroupe les redémarrages en rafale en un repère « n fois »", () => {
    const buffer = new LogBuffer();
    buffer.append([
      { kind: "restart", at: 1_000 },
      { kind: "restart", at: 1_400 },
      { kind: "restart", at: 1_900 },
      { kind: "restart", at: 9_000 },
    ]);
    const restarts = buffer.entries.filter(
      (e): e is Extract<Entry, { kind: "restart" }> => e.kind === "restart",
    );
    expect(restarts.map((r) => r.count)).toEqual([3, 1]);
  });

  it("clear efface tout", () => {
    const buffer = new LogBuffer();
    buffer.append([line("secret")]);
    buffer.clear();
    expect(buffer.entries).toEqual([]);
    expect(buffer.removed).toBe(0);
  });

  it("après une reprise perdue, retire de la fin les lignes qui reviennent, et le trou avec elles", () => {
    const buffer = new LogBuffer();
    buffer.append([
      line("a", 1),
      line("b", 2),
      line("c", 3),
      { kind: "gap", gap: "missing", count: null },
    ]);
    const gap = buffer.lastGapId();
    expect(buffer.dropOverlap(2, gap)).toBe(2);
    expect(buffer.entries.map((e) => e.kind)).toEqual(["line"]);
    // Sans recouvrement : le trou reste.
    const other = new LogBuffer();
    other.append([line("a", 1), { kind: "gap", gap: "missing", count: null }]);
    expect(other.dropOverlap(5, other.lastGapId())).toBe(0);
    expect(other.entries).toHaveLength(2);
  });
});

describe("fenêtre : seulement ce qui est visible", () => {
  const metrics: Metrics = { lineHeight: 20, columns: 50, padding: 2, marker: 32 };
  const entries: Entry[] = Array.from({ length: 50_000 }, (_, id) => ({
    id,
    kind: "line",
    t: id,
    source: "out",
    text: "x".repeat(id % 7 === 0 ? 120 : 10),
    cut: null,
  }));

  it("estime la hauteur par le nombre de colonnes", () => {
    expect(linesFor(0, 50)).toBe(1);
    expect(linesFor(120, 50)).toBe(3);
    const first = entries[0] as Entry;
    expect(estimateHeight(first, metrics, 120)).toBe(3 * 20 + 4);
    expect(estimateHeight(first, metrics, 10, 1)).toBe(2 * 20 + 4);
    expect(estimateHeight({ id: 1, kind: "restart", at: 1, count: 1 }, metrics, 0)).toBe(32);
  });

  it("dessine une soixantaine d'entrées parmi cinquante mille, vite", () => {
    const heights = entries.map((entry) =>
      estimateHeight(entry, metrics, entry.kind === "line" ? entry.text.length : 0),
    );
    const started = performance.now();
    const { tops, total } = layout(heights);
    const [first, last] = visibleRange(tops, heights, total / 2, 600, metrics);
    const took = performance.now() - started;
    expect(last - first).toBeLessThan(100);
    expect(last - first).toBeGreaterThan(20);
    expect(took).toBeLessThan(1000);
    expect(indexAt(tops, heights, 0)).toBe(0);
    expect(visibleRange([], [], 0, 600, metrics)).toEqual([0, 0]);
  });

  it("« en bas » à 24 px près", () => {
    expect(atBottom(976, 600, 1600)).toBe(true);
    expect(atBottom(900, 600, 1600)).toBe(false);
  });
});

describe("recherche dans ce qui est chargé", () => {
  const entries: Entry[] = ["Erreur disque", "tout va bien", "ERREUR réseau"].map((text, id) => ({
    id,
    kind: "line",
    t: id,
    source: "out",
    text,
    cut: null,
  }));

  it("ignore la casse, ne lit jamais la requête comme un motif", () => {
    expect(findMatches(entries, "erreur").indices).toEqual([0, 2]);
    expect(findMatches(entries, ".*").indices).toEqual([]);
    expect(findMatches(entries, "  ").indices).toEqual([]);
  });

  it("marque les morceaux trouvés", () => {
    expect(segmentsOf("a Erreur b erreur", "erreur")).toEqual([
      { text: "a ", hit: false },
      { text: "Erreur", hit: true },
      { text: " b ", hit: false },
      { text: "erreur", hit: true },
    ]);
    expect(segmentsOf("rien", "")).toEqual([{ text: "rien", hit: false }]);
  });
});

describe("heures", () => {
  it("heure du PC, heure inconnue sans mentir", () => {
    const t = new Date(2026, 9, 13, 14, 3, 22, 481).getTime();
    expect(clockOf(t)).toBe("14:03:22");
    expect(fullTimeOf(t)).toBe("13 octobre 2026, 14:03:22.481");
    expect(clockOf(null)).toBe("--:--:--");
    expect(fullTimeOf(Number.NaN)).toBeNull();
  });
});

describe("avis : valeur inconnue = texte neutre, jamais un état affirmé", () => {
  const base = { failure: null, hasLines: false, remainingS: 0 } as const;

  it("plafond du compte et frein", () => {
    const account = endNotice({
      ...base,
      end: { kind: "refused", refusal: "logs_account_limit", retryAfterS: null },
    });
    expect(account?.text).toBe(
      "Tu lis déjà les journaux de 3 services. Ferme un onglet Journaux pour en ouvrir un autre.",
    );
    expect(account?.retry).toBe("now");
    const brake = endNotice({
      ...base,
      remainingS: 4,
      end: { kind: "refused", refusal: "logs_rate_limited", retryAfterS: 4 },
    });
    expect(brake?.text).toContain("Attends 4 s");
    expect(brake?.retry).toBe("wait");
    expect(
      endNotice({
        ...base,
        remainingS: 1,
        end: { kind: "refused", refusal: "logs_rate_limited", retryAfterS: 1 },
      })?.text,
    ).toContain("Attends 1 s");
    expect(
      endNotice({ ...base, end: { kind: "refused", refusal: "logs_rate_limited", retryAfterS: 4 } })
        ?.text,
    ).toBe("Tu peux réessayer.");
  });

  it("refus sans le droit, service disparu, moteur", () => {
    expect(
      endNotice({ ...base, end: { kind: "refused", refusal: "missing_right", retryAfterS: null } })
        ?.lock,
    ).toBe(true);
    const gone = endNotice({
      ...base,
      hasLines: true,
      end: { kind: "refused", refusal: "service_not_found", retryAfterS: null },
    });
    expect(gone?.text).toContain("Les lignes affichées sont les dernières reçues");
    expect(gone?.stale).toBe(true);
    expect(
      endNotice({
        ...base,
        end: { kind: "refused", refusal: "engine_unavailable", retryAfterS: null },
      })?.tone,
    ).toBe("warn");
  });

  it("un code inconnu est une fin générique, sans bouton fondé sur ce qu'il dirait", () => {
    const unknown = endNotice({
      ...base,
      end: { kind: "refused", refusal: "unknown", retryAfterS: null },
    });
    expect(unknown?.text).toBe("La lecture des journaux s'est arrêtée.");
    expect(unknown?.lock).toBe(false);
    expect(endNotice({ ...base, end: null })).toBeNull();
  });

  it("textes de trous, de compteur et de code de sortie", () => {
    expect(gapText("missing", null)).toContain("le moteur ne les a pas gardées");
    expect(gapText("missing", 1)).toBe("1 ligne manque ici");
    expect(gapText("pause", 12)).toBe("12 lignes n'ont pas été gardées pendant la pause.");
    expect(waitingText(1)).toBe("1 nouvelle ligne");
    expect(waitingText(12)).toBe("12 nouvelles lignes");
    expect(exitWords(137)).toContain("code 137");
    expect(exitWords(2)).toBe("code 2");
  });
});
