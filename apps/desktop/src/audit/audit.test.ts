import { afterEach, describe, expect, it } from "vitest";
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
import {
  attemptsOf,
  BURST_MIN_ATTEMPTS,
  BURST_WINDOW_MS,
  groupBursts,
  OVERFLOW_REASON_PREFIX,
} from "./grouping";
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

  it("le compte est EXACT : l'entrée de synthèse de l'agent EST la dernière occurrence, son repeat_count la compte déjà", () => {
    // L'agent : 3 refus de même clé dans la minute = la première entrée + UNE synthèse à repeat_count 2.
    const three = [entry({ at: T0 + 20_000, repeatCount: 2 }), entry({ at: T0 })];
    expect(three.reduce((sum, e) => sum + attemptsOf(e), 0)).toBe(3);
    // 3 vraies tentatives : sous le seuil de 5, pas de rafale (avec « 1 + repeat_count » on en aurait vu 4).
    expect(groupBursts(three).every((row) => row.kind === "entry")).toBe(true);
    // 5 vraies tentatives (1 + 4) : une rafale de 5, ni 4 ni 6.
    const five = [entry({ at: T0 + 20_000, repeatCount: 4 }), entry({ at: T0 })];
    const grouped = groupBursts(five);
    expect(grouped).toHaveLength(1);
    expect(grouped[0]?.kind === "burst" && grouped[0].attempts).toBe(5);
    // Plusieurs fenêtres de l'agent dans la rafale : aucune occurrence comptée deux fois ni oubliée.
    const windows = [
      entry({ at: T0 + 100_000, repeatCount: 7 }),
      entry({ at: T0 + 90_000 }),
      entry({ at: T0 + 40_000, repeatCount: 3 }),
      entry({ at: T0 + 10_000 }),
    ];
    const merged = groupBursts(windows);
    expect(merged).toHaveLength(1);
    expect(merged[0]?.kind === "burst" && merged[0].attempts).toBe(1 + 3 + 1 + 7);
  });

  it("l'adresse n'est affirmée que si elle est certaine : une synthèse (clé sans adresse) la retire du libellé", () => {
    const ordinary = groupBursts(refusals(5, 5000));
    expect(ordinary[0]?.kind === "burst" && ordinary[0].addr).toBe("203.0.113.9");
    const withSummary = groupBursts([
      entry({ at: T0 + 30_000, repeatCount: 4 }),
      entry({ at: T0 }),
    ]);
    expect(withSummary[0]?.kind).toBe("burst");
    expect(withSummary[0]?.kind === "burst" && withSummary[0].addr).toBeNull();
  });

  it("une synthèse dont la dernière occurrence vient d'une autre adresse n'entre pas dans la rafale d'une adresse", () => {
    const other = {
      kind: "client" as const,
      name: null,
      addr: "198.51.100.4",
      text: "198.51.100.4 (inconnu)",
    };
    const list = [entry({ at: T0 + 50_000, repeatCount: 9, origin: other }), ...refusals(5, 5000)];
    const grouped = groupBursts(list);
    expect(grouped[0]).toEqual({ kind: "entry", entry: list[0] });
    expect(grouped[1]?.kind).toBe("burst");
    expect(grouped[1]?.kind === "burst" && grouped[1].addr).toBe("203.0.113.9");
  });

  it("l'entrée de débordement de l'agent (« activité trop variée ») n'est jamais absorbée", () => {
    const overflow = entry({
      at: T0 + 30_000,
      repeatCount: 400,
      reason: `${OVERFLOW_REASON_PREFIX}, 400 événements regroupés`,
    });
    const list = [overflow, ...refusals(5, 5000)];
    const grouped = groupBursts(list);
    expect(grouped[0]).toEqual({ kind: "entry", entry: overflow });
    expect(grouped.filter((row) => row.kind === "burst")).toHaveLength(1);
  });

  it("un groupe à cheval sur deux pages : la rafale du bas n'est pas groupée tant qu'il en reste à charger, puis le compte est exact", () => {
    const all = refusals(8, 5000);
    const page1 = all.slice(0, 6);
    const page2 = all.slice(6);
    // Page 1 seule, suite à charger : un décompte partiel serait faux, on ne regroupe pas.
    expect(groupBursts(page1, false).every((row) => row.kind === "entry")).toBe(true);
    // Les deux pages : 8 tentatives exactes.
    const whole = groupBursts([...page1, ...page2], true);
    expect(whole).toHaveLength(1);
    expect(whole[0]?.kind === "burst" && whole[0].attempts).toBe(8);
    // Une rafale qui ne touche pas le bas est groupée même si la suite reste à charger.
    const calm = entry({ at: T0 - 60 * 60_000, outcome: "ok", reason: null });
    const above = groupBursts([...page1, calm], false);
    expect(above.some((row) => row.kind === "burst")).toBe(true);
  });

  it("à la limite d'affichage (suite à charger) rien ne prétend un compte partiel", () => {
    const list = refusals(30, 1000);
    expect(groupBursts(list, false).every((row) => row.kind === "entry")).toBe(true);
    expect(groupBursts(list, true).filter((row) => row.kind === "burst").length).toBeGreaterThan(0);
  });

  it("une réussite ou une action d'un autre type au milieu des refus n'est jamais absorbée et coupe la rafale", () => {
    const list = refusals(10, 3000);
    const logout = entry({
      at: T0 + 12_000,
      action: "logout",
      actionLabel: "Déconnexion",
      outcome: "ok",
      reason: null,
      account: "marie",
    });
    const mixed = [...list.slice(0, 5), logout, ...list.slice(5)];
    const grouped = groupBursts(mixed);
    const shownIds = grouped.flatMap((row) =>
      row.kind === "entry" ? [row.entry.id] : row.entries.map((e) => e.id),
    );
    expect(shownIds).toEqual(mixed.map((e) => e.id));
    expect(grouped.some((row) => row.kind === "entry" && row.entry.id === logout.id)).toBe(true);
    for (const row of grouped) {
      if (row.kind === "burst") expect(row.entries.every((e) => e.action === "login")).toBe(true);
    }
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

  it("l'état déployé survit à l'arrivée d'une entrée en tête et d'une page sous la rafale", () => {
    const list = refusals(6, 5000);
    const expanded = new Set(list.map((e) => e.id));
    const fresh = entry({ at: T0 + 25_000 });
    const withTop = displayRows([fresh, ...list], expanded);
    // La nouvelle tentative rejoint la rafale (même adresse, dans les 2 minutes) : elle reste déployée.
    expect(
      withTop
        .filter((row) => row.type === "burst")
        .every((row) => row.type === "burst" && row.expanded),
    ).toBe(true);
    const older = entry({ at: T0 - 5000 });
    const withBelow = displayRows([...list, older], expanded);
    expect(
      withBelow
        .filter((row) => row.type === "burst")
        .every((row) => row.type === "burst" && row.expanded),
    ).toBe(true);
  });

  it("déployer une rafale montre chaque tentative juste dessous, dans l'ordre", () => {
    const list = refusals(5, 5000);
    const closed = displayRows(list, new Set());
    const open = displayRows(list, new Set([list[0]?.id ?? 0]));
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

  it("« Aujourd'hui » commence à minuit local, 7 et 30 jours au début du jour d'il y a 7 ou 30 jours", () => {
    const today = resolveFilter(draft({ period: "today" }), NOW);
    expect(today?.fromS).toBe(Math.floor(new Date(2026, 9, 4, 0, 0, 0).getTime() / 1000));
    expect(today?.toS).toBeNull();
    expect(resolveFilter(draft({ period: "week" }), NOW)?.fromS).toBe(
      Math.floor(new Date(2026, 9, 4 - 7, 0, 0, 0).getTime() / 1000),
    );
    expect(resolveFilter(draft({ period: "month" }), NOW)?.fromS).toBe(
      Math.floor(new Date(2026, 9, 4 - 30, 0, 0, 0).getTime() / 1000),
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

describe("jours au calendrier du fuseau du PC (changements d'heure)", () => {
  const saved = process.env.TZ;
  afterEach(() => {
    if (saved === undefined) process.env.TZ = undefined as unknown as string;
    else process.env.TZ = saved;
    if (saved === undefined) Reflect.deleteProperty(process.env, "TZ");
  });

  const secondsOf = (y: number, m: number, d: number, h = 0, mi = 0, se = 0) =>
    Math.floor(new Date(y, m - 1, d, h, mi, se).getTime() / 1000);

  it("fin au jour du retour à l'heure d'hiver (25/10/2026, 25 h) : la dernière seconde du jour est incluse", () => {
    process.env.TZ = "Europe/Paris";
    const filter = resolveFilter(
      { ...emptyDraft(), period: "custom", fromDate: "2026-10-24", toDate: "2026-10-25" },
      new Date(2026, 9, 27, 12).getTime(),
    );
    expect(filter?.toS).toBe(secondsOf(2026, 10, 25, 23, 59, 59));
    // Le jour dure 25 h : de minuit local à minuit local, 25 * 3600 secondes.
    expect((filter?.toS ?? 0) + 1 - secondsOf(2026, 10, 25)).toBe(25 * 3600);
  });

  it("fin au jour du passage à l'heure d'été (29/03/2026, 23 h) : rien du lendemain n'est inclus", () => {
    process.env.TZ = "Europe/Paris";
    const filter = resolveFilter(
      { ...emptyDraft(), period: "custom", fromDate: "2026-03-28", toDate: "2026-03-29" },
      new Date(2026, 2, 31, 12).getTime(),
    );
    expect(filter?.toS).toBe(secondsOf(2026, 3, 29, 23, 59, 59));
    expect((filter?.toS ?? 0) + 1 - secondsOf(2026, 3, 29)).toBe(23 * 3600);
    expect((filter?.toS ?? 0) + 1).toBe(secondsOf(2026, 3, 30));
  });

  it("« Aujourd'hui », 7 jours, 30 jours et la limite de 90 jours tombent à minuit local malgré un changement d'heure", () => {
    process.env.TZ = "Europe/Paris";
    // Le 26/10/2026 : 7 jours avant, c'est le 19 ; 30 jours avant, le 26/09 ; 90 jours avant, le 28/07.
    const now = new Date(2026, 9, 26, 15, 30).getTime();
    expect(resolveFilter({ ...emptyDraft(), period: "today" }, now)?.fromS).toBe(
      secondsOf(2026, 10, 26),
    );
    expect(resolveFilter({ ...emptyDraft(), period: "week" }, now)?.fromS).toBe(
      secondsOf(2026, 10, 19),
    );
    expect(resolveFilter({ ...emptyDraft(), period: "month" }, now)?.fromS).toBe(
      secondsOf(2026, 9, 26),
    );
    expect(
      periodError({ ...emptyDraft(), period: "custom", fromDate: "2026-07-28" }, now),
    ).toBeNull();
    expect(periodError({ ...emptyDraft(), period: "custom", fromDate: "2026-07-27" }, now)).toBe(
      "tooOld",
    );
    // Le 29/03/2026 : le début du jour et le début de « il y a 7 jours » restent à minuit.
    const spring = new Date(2026, 2, 29, 10).getTime();
    expect(resolveFilter({ ...emptyDraft(), period: "week" }, spring)?.fromS).toBe(
      secondsOf(2026, 3, 22),
    );
  });

  it("à minuit pile : le jour qui commence est « aujourd'hui », celui qui finit s'arrête à 23:59:59", () => {
    process.env.TZ = "Europe/Paris";
    const midnight = new Date(2026, 9, 5, 0, 0, 0).getTime();
    expect(resolveFilter({ ...emptyDraft(), period: "today" }, midnight)?.fromS).toBe(
      secondsOf(2026, 10, 5),
    );
    const justBefore = new Date(2026, 9, 4, 23, 59, 59).getTime();
    expect(resolveFilter({ ...emptyDraft(), period: "today" }, justBefore)?.fromS).toBe(
      secondsOf(2026, 10, 4),
    );
  });

  it("le texte de recherche est borné comme celui de la coquille (200 caractères), sans erreur", () => {
    const filter = resolveFilter({ ...emptyDraft(), text: `  ${"é".repeat(300)}  ` }, Date.now());
    expect(Array.from(filter?.text ?? "")).toHaveLength(200);
  });
});
