import { describe, expect, it } from "vitest";
import type { FicheDetail } from "@/link";
import { rich, terraria } from "@/link/simulated-fiches";
import { GIB } from "@/resources";
import { keyFacts, needLines, portsFact } from "./facts";

// Les quatre faits clés et les phrases de « Ce qu'il faut pour ce jeu » (HRT-242, BR-FICHE-004, 057).
function withResources(detail: FicheDetail, patch: Partial<FicheDetail["resources"]>): FicheDetail {
  return { ...detail, resources: { ...detail.resources, ...patch } };
}

describe("les quatre faits clés", () => {
  it("sans aucun chiffre : mémoire et disque non vérifiés, un seul port, durée du premier démarrage", () => {
    const facts = keyFacts(terraria(), 64 * GIB, "forge");
    expect(facts.map((f) => f.id)).toEqual(["memory", "disk", "ports", "start"]);
    expect(facts[0]?.text).toBe("Mémoire nécessaire non vérifiée par Hearth");
    expect(facts[1]?.text).toBe("Espace disque non vérifié");
    expect(facts[2]?.text).toBe("Un seul port à ouvrir : 7777");
    expect(facts[3]?.text).toMatch(/^Premier démarrage : jusqu'à \d+ minutes$/);
  });

  it("avec des chiffres prouvés : la mémoire dit la machine, le disque dit la place", () => {
    const detail = withResources(terraria(), {
      memory: { minBytes: GIB, recommendedBytes: 2 * GIB, evidence: { kind: "measured" } },
      disk: { installBytes: 1.4 * GIB, evidence: { kind: "measured" } },
    });
    const facts = keyFacts(detail, 64 * GIB, "forge");
    expect(facts[0]?.text).toBe("2 Go de mémoire, forge en a 64 Go");
    expect(facts[1]?.text).toBe("1,4 Go d'espace disque");
  });

  it("plusieurs ports : le nombre et la liste ; la console jamais publiée n'est pas comptée", () => {
    const detail = rich();
    const published = detail.ports.filter((p) => p.publish !== "never").length;
    const text = portsFact(detail);
    if (published > 1) expect(text).toMatch(new RegExp(`^${published} ports à ouvrir : `));
    else expect(text).toMatch(/^(Un seul port à ouvrir|Aucun port à ouvrir)/);
    expect(portsFact({ ...detail, ports: [] })).toBe("Aucun port à ouvrir");
  });
});

describe("les lignes de « Place sur votre serveur »", () => {
  it("disent le besoin, la machine et la preuve", () => {
    const detail = withResources(terraria(), {
      memory: { minBytes: 2 * GIB, recommendedBytes: 2 * GIB, evidence: { kind: "measured_idle" } },
    });
    const lines = needLines(detail, { memoryBytes: 64 * GIB, cpus: 16 }, "forge");
    expect(lines.memory).toBe(
      "Mémoire : il faut 2 Go, forge en a 64 Go. Mesuré sans joueur : il en faut plus avec des joueurs.",
    );
    expect(lines.cpu).toBe(
      "Processeur : non vérifié. Hearth ne peut pas dire combien ce jeu en demande ; forge a 16 cœurs.",
    );
  });

  it("sans preuve, aucun nombre", () => {
    const lines = needLines(terraria(), null, "forge");
    expect(lines.memory).toMatch(/non vérifiée/);
    expect(lines.disk).toMatch(/non vérifié/);
    expect(lines.memory).not.toMatch(/\d/);
  });
});
