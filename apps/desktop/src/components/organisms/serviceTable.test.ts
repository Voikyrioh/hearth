import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { nextTick } from "vue";
import type { ServiceInfo } from "@/link";
import ServiceTable from "./ServiceTable.vue";

function service(patch: Partial<ServiceInfo> & { key: string }): ServiceInfo {
  return {
    name: patch.key,
    origin: "external",
    driver: null,
    unit: null,
    removedOnStop: false,
    composeProject: null,
    state: "running",
    since: null,
    createdAt: null,
    neverStarted: false,
    exitCode: null,
    restarts10m: 0,
    image: { ref: "nginx", tag: "1.27", shortId: "0123456789ab" },
    ports: [],
    portsOmitted: 0,
    hostNetwork: false,
    ...patch,
  };
}

const many = (count: number) =>
  Array.from({ length: count }, (_, index) =>
    service({
      key: `c_${String(index).padStart(4, "0")}`,
      name: `service-${String(index).padStart(3, "0")}`,
    }),
  );

describe("ServiceTable : liste virtualisée et accessible (BR-SVC-019, 021)", () => {
  it("200 services : une grille ARIA exacte, mais seules les lignes visibles et quelques voisines dans le DOM", () => {
    const wrapper = mount(ServiceTable, { props: { services: many(200) } });
    const grid = wrapper.get("[role='grid']");
    expect(grid.attributes("aria-rowcount")).toBe("201");
    expect(grid.attributes("aria-label")).toBe("Liste des services");
    expect(wrapper.findAll("[role='columnheader']").map((h) => h.text())).toEqual([
      "Nom",
      "Image",
      "État",
      "Ports",
    ]);
    const rows = wrapper.findAll("[data-row]");
    expect(rows.length).toBeGreaterThan(5);
    expect(rows.length).toBeLessThan(40);
    expect(rows[0]?.attributes("aria-rowindex")).toBe("2");
    wrapper.unmount();
  });

  it("un seul arrêt de tabulation ; les flèches, Fin et Début déplacent la ligne active par clé", async () => {
    const wrapper = mount(ServiceTable, {
      props: { services: many(200) },
      attachTo: document.body,
    });
    const tabbable = () =>
      wrapper.findAll("[data-row]").filter((row) => row.attributes("tabindex") === "0");
    expect(tabbable().map((row) => row.attributes("data-key"))).toEqual(["c_0000"]);
    const grid = wrapper.get("[role='grid']");
    await grid.trigger("keydown", { key: "ArrowDown" });
    await flushPromises();
    expect(tabbable().map((row) => row.attributes("data-key"))).toEqual(["c_0001"]);
    expect(document.activeElement?.getAttribute("data-key")).toBe("c_0001");
    await grid.trigger("keydown", { key: "End" });
    await flushPromises();
    expect(tabbable().map((row) => row.attributes("data-key"))).toEqual(["c_0199"]);
    await grid.trigger("keydown", { key: "Home" });
    await flushPromises();
    expect(tabbable().map((row) => row.attributes("data-key"))).toEqual(["c_0000"]);
    // Aucun texte tronqué ne prend le focus : la ligne est le seul arrêt.
    expect(wrapper.findAll("[data-middle][tabindex]")).toHaveLength(0);
    wrapper.unmount();
  });

  it("le focus suit la clé, pas l'index : une ligne insérée au-dessus ne le déplace pas", async () => {
    const wrapper = mount(ServiceTable, {
      props: { services: many(5) },
      attachTo: document.body,
    });
    await wrapper.get("[role='grid']").trigger("keydown", { key: "ArrowDown" });
    await flushPromises();
    expect(document.activeElement?.getAttribute("data-key")).toBe("c_0001");
    await wrapper.setProps({
      services: [service({ key: "c_new", name: "a-nouveau" }), ...many(5)],
    });
    await flushPromises();
    expect(document.activeElement?.getAttribute("data-key")).toBe("c_0001");
    expect(
      wrapper
        .findAll("[data-row]")
        .find((row) => row.attributes("tabindex") === "0")
        ?.attributes("data-key"),
    ).toBe("c_0001");
    wrapper.unmount();
  });

  it("un nom, une image et un chemin de 255 caractères restent dans le document, complets pour un lecteur d'écran", () => {
    const name = `n${"a".repeat(253)}z`;
    const ref = `registre.exemple/${"b".repeat(236)}`;
    const wrapper = mount(ServiceTable, {
      props: {
        services: [service({ key: "c_long", name, image: { ref, tag: "9.9", shortId: "x" } })],
      },
    });
    expect(name).toHaveLength(255);
    const row = wrapper.get("[data-row]");
    expect(row.text()).toContain(name);
    expect(row.text()).toContain(`${ref}:9.9`);
    expect(row.attributes("aria-label")).toContain(name);
    // Aucun style en ligne sur les lignes : la mise en page vient des jetons.
    expect(row.attributes("style")).toBeUndefined();
    wrapper.unmount();
  });

  it("repères d'origine : « Créé hors de Hearth » seulement hors de Hearth, « Géré par le système » et la pile Compose à part", () => {
    const wrapper = mount(ServiceTable, {
      props: {
        services: [
          service({ key: "a", name: "a", origin: "external" }),
          service({ key: "b", name: "b", origin: "hearth" }),
          service({
            key: "c",
            name: "c",
            origin: "hearth",
            driver: "supervisor",
            unit: "docker-c.service",
            composeProject: "shop",
          }),
        ],
      },
    });
    const text = (key: string) => wrapper.get(`[data-key='${key}']`).text();
    expect(text("a")).toContain("Créé hors de Hearth");
    expect(text("b")).not.toContain("Créé hors de Hearth");
    expect(text("c")).toContain("Géré par le système (docker-c.service)");
    expect(text("c")).toContain("Pile Compose shop");
    wrapper.unmount();
  });

  it("le défilement ne saute pas quand une ligne au-dessus passe en boucle (BR-SVC-015)", async () => {
    const wrapper = mount(ServiceTable, { props: { services: many(60) }, attachTo: document.body });
    const scroller = wrapper.get(".table__scroll").element as HTMLElement;
    scroller.scrollTop = 52 * 20 + 10;
    await scroller.dispatchEvent(new Event("scroll"));
    await nextTick();
    const changed = many(60).map((item, index) =>
      index === 3 ? { ...item, state: "restart_loop" as const, restarts10m: 4, exitCode: 1 } : item,
    );
    await wrapper.setProps({ services: changed });
    await flushPromises();
    // La ligne 20 était à 10 px du haut : elle y reste malgré les 28 px gagnés au-dessus.
    expect(scroller.scrollTop).toBe(52 * 20 + 28 + 10);
    wrapper.unmount();
  });
});

describe("ServiceTable : ports coupés par l'agent (BR-SVC-021)", () => {
  const port = (hostPort: number) => ({
    hostPort,
    containerPort: 80,
    protocol: "tcp" as const,
    scope: "all" as const,
  });
  const cell = (wrapper: ReturnType<typeof mount>) => wrapper.get(".cell--ports");

  it("le « +N » compte les ports coupés sans les lister ; le survol le dit", () => {
    const wrapper = mount(ServiceTable, {
      props: {
        services: [service({ key: "a", ports: [port(1), port(2)], portsOmitted: 2872 })],
      },
    });
    expect(cell(wrapper).text()).toContain("+2872");
    expect(cell(wrapper).attributes("title")).toBe(
      "1 vers 80 (TCP), 2 vers 80 (TCP), et 2872 autres non listés",
    );
    wrapper.unmount();
  });

  it("des ports listés qui tiennent et des ports coupés : « +N » des seuls ports coupés", () => {
    const wrapper = mount(ServiceTable, {
      props: { services: [service({ key: "a", ports: [port(1)], portsOmitted: 5 })] },
    });
    expect(cell(wrapper).text()).toContain("+5");
    wrapper.unmount();
  });

  it("aucun port listé mais des ports coupés : jamais « Aucun port »", () => {
    const wrapper = mount(ServiceTable, {
      props: { services: [service({ key: "a", ports: [], portsOmitted: 7 })] },
    });
    expect(cell(wrapper).text()).toContain("+7");
    expect(cell(wrapper).text()).not.toContain("Aucun port");
    expect(cell(wrapper).attributes("title")).toBe("7 non listés");
    wrapper.unmount();
  });

  it("rien de coupé : comportement d'avant (pas de survol quand tout tient)", () => {
    const wrapper = mount(ServiceTable, {
      props: { services: [service({ key: "a", ports: [port(1)] })] },
    });
    expect(cell(wrapper).attributes("title")).toBeUndefined();
    expect(cell(wrapper).text()).not.toContain("+");
    wrapper.unmount();
  });
});
