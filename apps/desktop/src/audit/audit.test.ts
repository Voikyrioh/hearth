import { describe, expect, it } from "vitest";
import type { AuditEntry, AuditFilter } from "@/link";
import {
  emptyDraft,
  type FilterDraft,
  hasAnyFilter,
  isUnfiltered,
  parseLocalDay,
  periodError,
  resolveFilter,
  sameDraft,
} from "./filters";
import { formatWhen } from "./format";
import { BURST_MIN_ATTEMPTS, BURST_WINDOW_MS, groupBursts } from "./grouping";
import { displayRows } from "./rows";
import { CELL_MAX_CHARS, CONTROL_MARK, clip, safeText } from "./text";

// BR-AUDIT-012 à 016 côté interface : les règles PURES (regroupement, filtres, texte non fiable,
// fuseau). Le filtrage lui-même est celui de l'agent : il n'est jamais recopié ici.

const T0 = Date.UTC(2026, 9, 4, 10, 0, 0);

let nextId = 1;
function entry(over: Omit<Partial<AuditEntry>, "at" | "atMs"> & { at?: number } = {}): AuditEntry {
  const { at, ...rest } = over;
  const atMs = at ?? T0;
  return {
    id: nextId++,
    at: new Date(atMs).toISOString(),
    atMs,
    account: null,
    origin: { kind: "client", name: null, addr: "203.0.113.9", text: "203.0.113.9 (inconnu)" },
    action: "login",
    actionLabel: "Connexion",
    target: null,
    outcome: "denied",
    reason: "identifiants incorrects",
    repeatCount: 0,
    ...rest,
  };
}

/** `n` refus de connexion, un toutes les `every` ms, rendus de la plus récente à la plus ancienne. */
function refusals(
  n: number,
  every = 10_000,
  over: Omit<Partial<AuditEntry>, "at" | "atMs"> = {},
): AuditEntry[] {
  const list = Array.from({ length: n }, (_, i) => entry({ at: T0 + i * every, ...over }));
  return list.reverse();
}

describe("regroupement des rafales (BR-AUDIT-013)", () => {
  it("regroupe au moins 5 refus de la même adresse en 2 minutes : « X tentatives refusées en Y min »", () => {
    const grouped = groupBursts(refusals(6, 15_000));
    expect(grouped).toHaveLength(1);
    const [burst] = grouped;
    expect(burst?.kind).toBe("burst");
    if (burst?.kind !== "burst") return;
    expect(burst.attempts).toBe(6);
    expect(burst.entries).toHaveLength(6);
    expect(burst.minutes).toBe(2);
    expect(burst.addr).toBe("203.0.113.9");
  });

  it("ne regroupe pas moins de 5 tentatives, ni 5 tentatives trop étalées", () => {
    expect(groupBursts(refusals(4)).every((row) => row.kind === "entry")).toBe(true);
    const spread = groupBursts(refusals(5, BURST_WINDOW_MS)); // une par fenêtre entière
    expect(spread.every((row) => row.kind === "entry")).toBe(true);
    expect(BURST_MIN_ATTEMPTS).toBe(5);
  });

  it("un regroupement ne masque JAMAIS une entrée d'un autre type, d'une autre adresse ou d'un autre résultat : elle coupe la rafale", () => {
    const list = refusals(6, 5000);
    const success = entry({
      at: T0 + 12_000,
      outcome: "ok",
      reason: null,
      account: "marie",
      origin: { kind: "client", name: "poste", addr: "203.0.113.9", text: "203.0.113.9 (poste)" },
    });
    // La réussite s'intercale (les refus sont rangés du plus récent au plus ancien).
    const mixed = [...list.slice(0, 3), success, ...list.slice(3)];
    const grouped = groupBursts(mixed);
    const shown = grouped.flatMap((row) => (row.kind === "entry" ? [row.entry] : row.entries));
    expect(shown.map((e) => e.id)).toEqual(mixed.map((e) => e.id));
    expect(grouped.some((row) => row.kind === "entry" && row.entry.id === success.id)).toBe(true);
    // Chaque moitié (3 refus) est trop courte pour une rafale.
    expect(grouped.every((row) => row.kind === "entry")).toBe(true);
    const otherAddr = refusals(6, 5000);
    otherAddr[2] = entry({
      at: T0,
      origin: { kind: "client", name: null, addr: "198.51.100.4", text: "198.51.100.4 (inconnu)" },
    });
    expect(groupBursts(otherAddr).some((row) => row.kind === "burst")).toBe(false);
  });

  it("ne regroupe jamais le blocage temporaire ni une action autre que la connexion", () => {
    const list = [
      entry({ at: T0 + 60_000, action: "login.locked", actionLabel: "Blocage temporaire" }),
      ...refusals(5, 5000),
    ];
    const grouped = groupBursts(list);
    const kinds = grouped.map((row) => row.kind);
    expect(kinds).toEqual(["entry", "burst"]);
    const locked = grouped[0];
    expect(locked?.kind === "entry" && locked.entry.action).toBe("login.locked");
  });

  it("compte pour 1 + N les répétitions que l'agent a déjà condensées", () => {
    const list = [
      entry({ at: T0 + 20_000, repeatCount: 3 }),
      entry({ at: T0 + 10_000, repeatCount: 0 }),
      entry({ at: T0, repeatCount: 0 }),
    ];
    const grouped = groupBursts(list);
    expect(grouped).toHaveLength(1);
    expect(grouped[0]?.kind === "burst" && grouped[0].attempts).toBe(6);
  });

  it("une seule entrée de synthèse n'est pas un regroupement : elle s'affiche seule avec sa raison", () => {
    const grouped = groupBursts([entry({ repeatCount: 50 })]);
    expect(grouped).toEqual([expect.objectContaining({ kind: "entry" })]);
  });

  it("une nouvelle tentative en tête ne change pas la composition des rafales plus anciennes", () => {
    const old = refusals(6, 10_000);
    const before = groupBursts(old);
    const fresh = entry({ at: T0 + 60 * 60_000 });
    const after = groupBursts([fresh, ...old]);
    expect(after[0]).toEqual({ kind: "entry", entry: fresh });
    expect(after[1]).toEqual(before[0]);
  });

  it("déployer une rafale montre chaque tentative juste dessous, dans l'ordre", () => {
    const list = refusals(5, 5000);
    const grouped = groupBursts(list);
    const key = grouped[0]?.kind === "burst" ? grouped[0].key : "";
    const closed = displayRows(list, new Set());
    const open = displayRows(list, new Set([key]));
    expect(closed).toHaveLength(1);
    expect(open).toHaveLength(6);
    expect(open.slice(1).map((row) => (row.type === "entry" ? row.entry.id : 0))).toEqual(
      list.map((e) => e.id),
    );
  });
});

describe("filtres (BR-AUDIT-014, 015)", () => {
  const NOW = new Date(2026, 9, 4, 15, 30, 0).getTime();

  function draft(over: Partial<FilterDraft>): FilterDraft {
    return { ...emptyDraft(), ...over };
  }

  it("un brouillon vide ne retient rien ; effacer revient à cet état", () => {
    const filter = resolveFilter(emptyDraft(), NOW) as AuditFilter;
    expect(isUnfiltered(filter)).toBe(true);
    expect(hasAnyFilter(emptyDraft())).toBe(false);
    expect(hasAnyFilter(draft({ text: "  marie " }))).toBe(true);
    expect(hasAnyFilter(draft({ text: "   " }))).toBe(false);
  });

  it("« Aujourd'hui » commence à minuit local, les périodes glissantes à maintenant moins 7 ou 30 jours", () => {
    const today = resolveFilter(draft({ period: "today" }), NOW);
    expect(today?.fromS).toBe(Math.floor(new Date(2026, 9, 4, 0, 0, 0).getTime() / 1000));
    expect(today?.toS).toBeNull();
    expect(resolveFilter(draft({ period: "week" }), NOW)?.fromS).toBe(
      Math.floor((NOW - 7 * 86_400_000) / 1000),
    );
    expect(resolveFilter(draft({ period: "month" }), NOW)?.fromS).toBe(
      Math.floor((NOW - 30 * 86_400_000) / 1000),
    );
  });

  it("une période personnalisée inclut la dernière seconde du jour de fin", () => {
    const filter = resolveFilter(
      draft({ period: "custom", fromDate: "2026-10-01", toDate: "2026-10-03" }),
      NOW,
    );
    expect(filter?.fromS).toBe(Math.floor(new Date(2026, 9, 1, 0, 0, 0).getTime() / 1000));
    expect(filter?.toS).toBe(Math.floor(new Date(2026, 9, 3, 23, 59, 59).getTime() / 1000));
  });

  it("refuse une fin avant le début et une date au-delà de 90 jours (messages de la spec)", () => {
    expect(
      periodError(draft({ period: "custom", fromDate: "2026-10-03", toDate: "2026-10-01" }), NOW),
    ).toBe("reversed");
    expect(periodError(draft({ period: "custom", fromDate: "2026-06-01" }), NOW)).toBe("tooOld");
    expect(periodError(draft({ period: "custom", toDate: "2026-06-01" }), NOW)).toBe("tooOld");
    expect(periodError(draft({ period: "custom", fromDate: "2026-02-31" }), NOW)).toBe(
      "incomplete",
    );
    expect(resolveFilter(draft({ period: "custom", fromDate: "2026-06-01" }), NOW)).toBeNull();
    // La limite : il y a exactement 90 jours est encore permis.
    const limit = new Date(2026, 9, 4 - 90);
    const text = `${limit.getFullYear()}-${String(limit.getMonth() + 1).padStart(2, "0")}-${String(limit.getDate()).padStart(2, "0")}`;
    expect(periodError(draft({ period: "custom", fromDate: text }), NOW)).toBeNull();
    // Hors période personnalisée, des dates restées dans les champs ne comptent pas.
    expect(periodError(draft({ period: "week", fromDate: "1999-01-01" }), NOW)).toBeNull();
  });

  it("garde l'ordre des listes fermées et ne dépend pas de l'ordre de sélection", () => {
    const filter = resolveFilter(
      draft({ kinds: ["denied", "login_ok"], outcomes: ["failed", "ok"], accounts: ["paul"] }),
      NOW,
    );
    expect(filter?.kinds).toEqual(["login_ok", "denied"]);
    expect(filter?.outcomes).toEqual(["ok", "failed"]);
    expect(filter?.accounts).toEqual(["paul"]);
  });

  it("le bouton « Appliquer » attend une vraie différence", () => {
    const a = draft({ kinds: ["login_ok", "update"] });
    expect(sameDraft(a, draft({ kinds: ["update", "login_ok"] }))).toBe(true);
    expect(sameDraft(a, draft({ kinds: ["login_ok"] }))).toBe(false);
    expect(sameDraft(draft({ text: "x " }), draft({ text: "x" }))).toBe(true);
    expect(parseLocalDay("2026-10-04")).not.toBeNull();
    expect(parseLocalDay("04/10/2026")).toBeNull();
  });
});

describe("texte non fiable", () => {
  it("rend inoffensifs les caractères de contrôle, sauts de ligne et inversions de sens de lecture", () => {
    const hostile = `a${String.fromCharCode(0)}b\nc\td${String.fromCharCode(0x202e)}e${String.fromCharCode(0x2028)}f${String.fromCharCode(0x9b)}`;
    const safe = safeText(hostile);
    expect(safe).toBe(["a", "b", "c", "d", "e", "f", ""].join(CONTROL_MARK));
    expect(safeText(null)).toBe("");
    expect(safeText("<img src=x onerror=alert(1)>")).toBe("<img src=x onerror=alert(1)>");
  });

  it("tronque proprement, sans couper une paire de substitution, et dit qu'il y a une suite", () => {
    const long = "x".repeat(CELL_MAX_CHARS + 20);
    const cut = clip(long);
    expect(cut.truncated).toBe(true);
    expect(Array.from(cut.text)).toHaveLength(CELL_MAX_CHARS);
    const emoji = "\u{1F600}".repeat(10);
    const clipped = clip(emoji, 5);
    expect(clipped.text.startsWith("\u{1F600}".repeat(4))).toBe(true);
    expect(clip("court").truncated).toBe(false);
  });
});

describe("fuseau horaire (BR-AUDIT-012)", () => {
  it("affiche l'heure dans le fuseau du PC ; la source UTC n'est pas touchée", () => {
    const at = Date.UTC(2026, 9, 4, 10, 30, 15);
    expect(formatWhen(at, "UTC")).toBe("04/10/2026 10:30:15");
    expect(formatWhen(at, "Europe/Paris")).toBe("04/10/2026 12:30:15");
    expect(formatWhen(at, "America/New_York")).toBe("04/10/2026 06:30:15");
    expect(formatWhen(Number.NaN)).toBeNull();
  });
});
