import { describe, expect, it } from "vitest";
import { service } from "@/test/service";
import { categoryOf, countServices, shownCounts, visibleUnder } from "./counts";
import { STATE_KEYS } from "./format";
import { pilotLock } from "./pilot";

// Le dixième état « Modification en cours » (BR-SVC-004, BR-SVC-011, BR-ENGINE-004, HRT-84) et le verrou des
// boutons de pilotage (BR-SVC-031).
describe("« Modification en cours »", () => {
  it("est compté dans « en transition »", () => {
    expect(categoryOf("updating")).toBe("transition");
    const counts = countServices([
      service({ key: "a", state: "running" }),
      service({ key: "b", state: "updating" }),
      service({ key: "c", state: "deploying" }),
    ]);
    expect(counts.transition).toBe(2);
    expect(shownCounts(counts).find((count) => count.key === "transition")?.n).toBe(2);
  });

  it("n'est visible que sous « Tous » (BR-SVC-011)", () => {
    for (const filter of ["running", "stopped", "error"] as const) {
      expect(visibleUnder(filter, "updating")).toBe(false);
    }
    expect(visibleUnder("all", "updating")).toBe(true);
  });

  it("reste visible sous le filtre actif tant que l'action de l'utilisateur court", () => {
    expect(visibleUnder("running", "updating", true)).toBe(true);
  });

  it("« En erreur » regroupe l'erreur et la boucle ; une pause n'est visible que sous « Tous »", () => {
    expect(visibleUnder("error", "restart_loop")).toBe(true);
    expect(visibleUnder("running", "paused")).toBe(false);
    expect(visibleUnder("all", "paused")).toBe(true);
  });

  it("a son texte et reste la dixième valeur de la liste fermée", () => {
    expect(STATE_KEYS.updating).toBe("services.state.updating");
    expect(Object.keys(STATE_KEYS)).toHaveLength(10);
  });
});

describe("le verrou de pilotage", () => {
  it("un service tenu par une action est verrouillé, avec qui l'a lancée", () => {
    const busy = { kind: "update", by: "marie", since: 1 };
    expect(pilotLock({ busy })).toEqual({ locked: true, by: "Lancée par marie." });
  });

  it("sans indication de l'agent, le service est verrouillé sans nom", () => {
    expect(pilotLock({ busy: null }, true)).toEqual({ locked: true, by: null });
  });

  it("un service libre n'est pas verrouillé", () => {
    expect(pilotLock({ busy: null })).toEqual({ locked: false, by: null });
  });

  it("un compte dont l'identifiant est illisible n'est pas nommé", () => {
    expect(pilotLock({ busy: { kind: "update", by: "<img>", since: null } })).toEqual({
      locked: true,
      by: null,
    });
  });
});
