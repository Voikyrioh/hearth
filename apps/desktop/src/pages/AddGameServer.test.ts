import { describe, expect, it } from "vitest";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";
import { boot, settle, useDeployTestClock } from "@/test/deploy";

// « Ajouter un serveur de jeu » (HRT-243, BR-DEPLOY-001, 039) : l'écran des trois chemins, sa garde et ses boutons.
useDeployTestClock();

const ADD = "/servers/forge/services/ajouter";

describe("l'écran des trois chemins (BR-DEPLOY-039)", () => {
  it("propose le Catalogue, un programme du choix, un serveur déjà présent, dans cet ordre", async () => {
    const { wrapper, router } = await boot(undefined, ADD);
    expect(router.currentRoute.value.name).toBe("services-add");
    expect(wrapper.get("h1").text()).toBe("Ajouter un serveur de jeu");
    const titles = wrapper.findAll("[data-add-path] h2").map((item) => item.text());
    expect(titles).toEqual([
      "Choisir un jeu dans le Catalogue",
      "Installer un programme de votre choix",
      "J'ai déjà un serveur de jeu sur forge",
    ]);
    const tags = wrapper.findAll("[data-add-path-tag]").map((item) => item.text());
    expect(tags).toEqual(["Conseillé", "Pour les habitués"]);
    // L'entrée « Services » reste marquée.
    expect(wrapper.get(".nav__item.router-link-active").text()).toBe("Services");
    wrapper.unmount();
  });

  it("ne crée rien et dit l'état du droit et du moteur en pied", async () => {
    const { wrapper, bridge } = await boot(undefined, ADD);
    const footer = wrapper.get("[data-add-state]").text();
    expect(footer).toContain("Vous avez le droit de gérer les services de forge.");
    expect(footer).toContain("Le moteur de conteneurs est prêt.");
    expect(footer).toContain(
      "Vous pourrez revenir sur votre saisie à tout moment avant de lancer.",
    );
    expect(bridge.deploys.running("forge")).toEqual([]);
    wrapper.unmount();
  });

  it("« Ouvrir le Catalogue » ouvre le Catalogue", async () => {
    const { wrapper, router } = await boot(undefined, ADD);
    await wrapper.get("[data-add-path-catalog] [data-add-path-go]").trigger("click");
    await settle();
    expect(router.currentRoute.value.name).toBe("catalogue");
    wrapper.unmount();
  });

  it("« Continuer sans fiche » ouvre le formulaire d'image, moteur prêt", async () => {
    const { wrapper, router } = await boot(undefined, ADD);
    const free = wrapper.get("[data-add-path-free] [data-add-path-go]");
    expect(free.attributes("aria-disabled")).toBeUndefined();
    await free.trigger("click");
    await settle();
    expect(router.currentRoute.value.name).toBe("services-deploy");
    wrapper.unmount();
  });

  it("« Voir comment faire » ouvre l'aide « J'ai déjà un serveur de jeu »", async () => {
    const { wrapper, router } = await boot(undefined, ADD);
    await wrapper.get("[data-add-path-existing] [data-add-path-go]").trigger("click");
    await settle();
    expect(router.currentRoute.value.name).toBe("existing-server-help");
    wrapper.unmount();
  });

  it("« Fermer » revient à la liste des services", async () => {
    const { wrapper, router } = await boot(undefined, ADD);
    await wrapper.get("[data-add-close]").trigger("click");
    await settle();
    expect(router.currentRoute.value.name).toBe("services");
    wrapper.unmount();
  });
});

describe("le moteur et le droit", () => {
  it("moteur arrêté : l'écran s'ouvre, « Continuer sans fiche » est inactif avec sa raison, le reste actif", async () => {
    const { wrapper, router } = await boot(
      (bridge) => bridge.services.setEngine("forge", { state: "stopped" }),
      ADD,
    );
    expect(router.currentRoute.value.name).toBe("services-add");
    const free = wrapper.get("[data-add-path-free] [data-add-path-go]");
    expect(free.attributes("aria-disabled")).toBe("true");
    expect(wrapper.get("[data-add-path-free]").text()).toContain("moteur de conteneurs est arrêté");
    expect(
      wrapper.get("[data-add-path-catalog] [data-add-path-go]").attributes("aria-disabled"),
    ).toBeUndefined();
    expect(
      wrapper.get("[data-add-path-existing] [data-add-path-go]").attributes("aria-disabled"),
    ).toBeUndefined();
    expect(wrapper.get("[data-add-state]").text()).toContain(
      "Le moteur de conteneurs n'est pas prêt : seul le Catalogue et l'aide sont ouverts pour l'instant.",
    );
    wrapper.unmount();
  });

  it("renvoie vers la liste un compte sans le droit de gérer les services", async () => {
    const { wrapper, router } = await boot(
      (bridge) => bridge.setServiceRightOnServer("forge", "marie", false),
      ADD,
    );
    expect(router.currentRoute.value.name).toBe("services");
    wrapper.unmount();
  });

  it("renvoie vers la liste un moteur absent ou non pris en charge", async () => {
    for (const state of ["absent", "unsupported_engine", "unsupported_version"] as const) {
      const { wrapper, router } = await boot(
        (bridge) => bridge.services.setEngine("forge", { state }),
        ADD,
      );
      expect(router.currentRoute.value.name, state).toBe("services");
      wrapper.unmount();
      document.body.innerHTML = "";
      useServicesStore().reset();
      useServiceRightsStore().reset();
    }
  });
});
