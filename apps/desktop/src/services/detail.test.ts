import { describe, expect, it } from "vitest";
import { service } from "@/test/service";
import {
  adoptedLine,
  commandText,
  detailsView,
  exitCodeText,
  graceText,
  mountView,
  openedService,
  reachText,
  restartText,
} from "./detail";

describe("openedService : le détail suit la CLÉ, jamais le nom (BR-SVC-016)", () => {
  const web = service({ key: "c_1", name: "web" });

  it("trouvé par sa clé", () => {
    expect(openedService([web], "c_1", true, null)).toEqual({ kind: "found", service: web });
  });

  it("liste pas encore connue : on attend, on ne dit pas « disparu »", () => {
    expect(openedService([], "c_1", false, null)).toEqual({ kind: "waiting" });
  });

  it("disparu : la dernière version reste, un homonyme est proposé mais jamais pris pour lui", () => {
    const again = service({ key: "c_2", name: "web" });
    expect(openedService([again], "c_1", true, web)).toEqual({
      kind: "gone",
      last: web,
      sameName: again,
    });
  });

  it("disparu sans l'avoir jamais vu : ni nom ni homonyme inventés", () => {
    const other = service({ key: "c_2", name: "web" });
    expect(openedService([other], "c_9", true, null)).toEqual({
      kind: "gone",
      last: null,
      sameName: null,
    });
  });

  it("un autre service au même nom n'est PAS le service ouvert", () => {
    const other = service({ key: "c_2", name: "web" });
    const result = openedService([other], "c_1", true, web);
    expect(result.kind).toBe("gone");
  });
});

describe("textes du détail", () => {
  it("code de sortie : trois explications, tout autre code seul (BR-SVC-023)", () => {
    expect(exitCodeText(0)).toBe("Code 0 : terminé normalement");
    expect(exitCodeText(137)).toBe("Code 137 : coupé de force (arrêt forcé ou manque de mémoire)");
    expect(exitCodeText(143)).toBe("Code 143 : arrêt demandé");
    expect(exitCodeText(2)).toBe("Code 2");
    expect(exitCodeText(-1)).toBe("Code -1");
  });

  it("redémarrage : les quatre phrases de la spec, une façon inconnue est dite sans être devinée", () => {
    expect(restartText("unless_stopped")).toBe("Redémarre tout seul sauf si vous l'arrêtez");
    expect(restartText("always")).toBe("Redémarre toujours");
    expect(restartText("none")).toBe("Ne redémarre pas tout seul");
    expect(restartText("supervisor")).toBe("Relancé par le système du serveur");
    expect(restartText("unknown")).toBe(
      "Hearth ne reconnaît pas la façon dont ce service redémarre.",
    );
    expect(restartText("jamais-vu" as never)).toBe(
      "Hearth ne reconnaît pas la façon dont ce service redémarre.",
    );
  });

  it("qui peut joindre un port : un périmètre inconnu ne s'écrit pas", () => {
    const port = { hostPort: 1, containerPort: 2, protocol: "tcp" as const };
    expect(reachText({ ...port, scope: "all" })).toBe(
      "Joignable depuis : Tout le réseau du serveur",
    );
    expect(reachText({ ...port, scope: "local" })).toBe("Joignable depuis : Ce serveur seulement");
    expect(reachText({ ...port, scope: "unknown" })).toBeNull();
  });

  it("dossiers : lecture seule ou non, un genre inconnu reste neutre", () => {
    expect(mountView({ kind: "folder", source: "/a", target: "/b", readOnly: true })).toEqual({
      kind: "folder",
      source: "/a",
      target: "/b",
      access: "Lecture seule",
    });
    expect(mountView({ kind: "volume", source: "v", target: "/b", readOnly: false }).access).toBe(
      "Lecture et écriture",
    );
    expect(mountView({ kind: "unknown", source: "s", target: "t", readOnly: false }).kind).toBe(
      "other",
    );
  });

  it("délai d'arrêt et commande", () => {
    expect(graceText(10)).toBe("10 secondes");
    expect(graceText(1)).toBe("1 seconde");
    expect(commandText(["nginx", "-g", "daemon off;"])).toBe("nginx -g daemon off;");
  });

  it("adopté par {compte} le {date} : le compte n'est cité que s'il a la forme d'un identifiant", () => {
    const date = () => "13/10/2026";
    const adopted = (by: string, at: number | null) => service({ key: "c", adopted: { by, at } });
    expect(adoptedLine(adopted("claire", 1), date)).toBe("Adopté par claire le 13/10/2026");
    expect(adoptedLine(adopted("<b>x</b>", 1), date)).toBe("Adopté le 13/10/2026");
    expect(adoptedLine(adopted("claire", null), date)).toBe("Adopté par claire");
    expect(adoptedLine(adopted("<b>x</b>", null), date)).toBeNull();
    expect(adoptedLine(service({ key: "c" }), date)).toBeNull();
  });
});

describe("detailsView : la commande et les noms ne sont montrés qu'au droit, et seulement s'ils ont été lus", () => {
  const web = service({ key: "c_1", name: "web" });
  const allowed = { kind: "allowed" } as const;

  it("visible : le droit est accordé et le serveur les a lus", () => {
    expect(detailsView(web, allowed)).toBe("visible");
  });

  it("réservé : un compte sans le droit, ou un service dont l'agent a retiré les détails", () => {
    expect(detailsView(web, { kind: "denied", isAdmin: false, command: null })).toBe("reserved");
    expect(detailsView(service({ key: "c_2", detailsReserved: true }), allowed)).toBe("reserved");
  });

  it("droit pas connu (lecture, échec, ancien agent, pilotage désactivé) : rien n'est montré", () => {
    for (const kind of ["reading", "failed", "old-agent", "engine-disabled"] as const) {
      expect(detailsView(web, { kind })).toBe("right-unknown");
    }
  });

  it("pas lus : une liste relue du disque, même avec le droit", () => {
    expect(detailsView(service({ key: "c_3", detailsKnown: false }), allowed)).toBe("not-read");
  });
});
