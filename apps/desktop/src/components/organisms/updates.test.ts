import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { formatAgo } from "@/composables/formatAgo";
import { useUpdatesStore } from "@/stores/updates";
import { SimulatedUpdateBridge, setUpdateBridge } from "@/updates";
import UpdateBanner from "./UpdateBanner.vue";
import UpdatePanel from "./UpdatePanel.vue";

const NEWER = { version: "1.1.0", notes: "Titre\n- une correction\n- une nouveauté" };

async function mountWith<T>(
  component: T,
  options: ConstructorParameters<typeof SimulatedUpdateBridge>[0] = {},
) {
  const pinia = createPinia();
  setActivePinia(pinia);
  const bridge = new SimulatedUpdateBridge(options);
  setUpdateBridge(bridge);
  const store = useUpdatesStore();
  await store.start();
  const wrapper = mount(component as never, {
    global: { plugins: [pinia] },
    attachTo: document.body,
  });
  await flushPromises();
  return { bridge, store, wrapper };
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
  // `<dialog>` n'a pas de `showModal` dans happy-dom.
  HTMLDialogElement.prototype.showModal = function showModal() {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function close() {
    this.removeAttribute("open");
  };
});
afterEach(() => {
  vi.useRealTimers();
  setUpdateBridge(null);
  document.body.innerHTML = "";
});

describe("UpdateBanner", () => {
  it("renders nothing when there is nothing to announce", async () => {
    const { wrapper } = await mountWith(UpdateBanner);
    expect(wrapper.find("[data-update-banner]").exists()).toBe(false);
  });

  it("announces the new version with the notes link and the two buttons", async () => {
    const { wrapper } = await mountWith(UpdateBanner, { available: NEWER });
    const banner = wrapper.get("[data-update-banner]");
    expect(banner.attributes("data-state")).toBe("available");
    expect(banner.attributes("role")).toBe("status");
    expect(banner.text()).toContain("Nouvelle version disponible");
    expect(banner.text()).toContain("1.1.0");
    const labels = banner.findAll("button").map((button) => button.text());
    expect(labels).toEqual(["Notes de version", "Plus tard", "Mettre à jour maintenant"]);
  });

  it("opens the release notes as plain text and closes them", async () => {
    const { wrapper } = await mountWith(UpdateBanner, {
      available: { version: "1.1.0", notes: "<b>gras</b>\n- ligne" },
    });
    await wrapper.findAll("button")[0]?.trigger("click");
    await flushPromises();
    const notes = document.body.querySelector("[data-release-notes]");
    expect(notes?.textContent).toBe("<b>gras</b>\n- ligne");
    expect(notes?.querySelector("b")).toBeNull(); // du texte, jamais du HTML
    expect(document.body.querySelector("dialog h2")?.textContent).toBe("Notes de version");
    (document.body.querySelector("dialog button") as HTMLButtonElement).click();
    await flushPromises();
    expect(document.body.querySelector("dialog")).toBeNull();
  });

  it("says so when a release has no notes", async () => {
    const { wrapper } = await mountWith(UpdateBanner, {
      available: { version: "1.1.0", notes: "" },
    });
    await wrapper.findAll("button")[0]?.trigger("click");
    await flushPromises();
    expect(document.body.querySelector("[data-release-notes]")?.textContent).toBe(
      "Aucune note pour cette version.",
    );
  });

  it("'Plus tard' makes the banner disappear", async () => {
    const { wrapper, bridge } = await mountWith(UpdateBanner, { available: NEWER });
    await wrapper.findAll("button")[1]?.trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-update-banner]").exists()).toBe(false);
    expect(bridge.calls.postpone).toBe(1);
  });

  it("'Mettre à jour maintenant' shows the download then the installation", async () => {
    const { wrapper, bridge } = await mountWith(UpdateBanner, { available: NEWER });
    await wrapper.findAll("button")[2]?.trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-update-banner]").attributes("data-state")).toBe("downloading");
    expect(wrapper.text()).toContain("Téléchargement en cours…");
    expect(wrapper.findAll("button")).toHaveLength(0);
    bridge.setProgress(35);
    await flushPromises();
    expect(wrapper.get("[data-update-progress]").text()).toBe("Téléchargement : 35 %");
    bridge.finishInstall();
    await flushPromises();
    expect(wrapper.text()).toContain("Installation en cours… Redémarrage du client…");
  });

  it("shows the exact message of an interrupted download, with retry and later", async () => {
    const { wrapper, bridge } = await mountWith(UpdateBanner, { available: NEWER });
    bridge.setInstallOutcome("interrupted");
    await wrapper.findAll("button")[2]?.trigger("click");
    bridge.finishInstall();
    await flushPromises();
    const banner = wrapper.get("[data-update-banner]");
    expect(banner.attributes("data-state")).toBe("failed");
    expect(banner.attributes("role")).toBe("alert");
    expect(banner.text()).toContain(
      "Téléchargement interrompu. La version en cours reste utilisable.",
    );
    expect(banner.findAll("button").map((b) => b.text())).toEqual(["Plus tard", "Réessayer"]);
    bridge.setInstallOutcome("ok");
    await banner.findAll("button")[1]?.trigger("click");
    bridge.finishInstall();
    await flushPromises();
    expect(wrapper.get("[data-update-banner]").attributes("data-state")).toBe("installing");
  });

  it("shows the exact message of a corrupted update", async () => {
    const { wrapper, bridge } = await mountWith(UpdateBanner, { available: NEWER });
    bridge.setInstallOutcome("corrupted");
    await wrapper.findAll("button")[2]?.trigger("click");
    bridge.finishInstall();
    await flushPromises();
    expect(wrapper.text()).toContain(
      "Mise à jour corrompue. Refusée. La version en cours reste utilisable.",
    );
  });

  it("shows a generic failure message for any other failure", async () => {
    const { wrapper, bridge } = await mountWith(UpdateBanner, { available: NEWER });
    bridge.setInstallOutcome("failed");
    await wrapper.findAll("button")[2]?.trigger("click");
    bridge.finishInstall();
    await flushPromises();
    expect(wrapper.text()).toContain(
      "La mise à jour n'a pas abouti. La version en cours reste utilisable.",
    );
  });

  it("uses no inline style", async () => {
    const { wrapper } = await mountWith(UpdateBanner, { available: NEWER });
    expect(wrapper.html()).not.toContain("style=");
  });
});

describe("UpdatePanel", () => {
  it("says the client is up to date with the age of the last check", async () => {
    const { wrapper } = await mountWith(UpdatePanel);
    expect(wrapper.get("h2").text()).toBe("Mises à jour");
    expect(wrapper.get("[data-updates-status]").text()).toBe("Tu es à jour");
    expect(wrapper.get("[data-updates-last]").text()).toBe(
      "Dernière vérification : il y a 2 heures",
    );
    expect(wrapper.get("button").text()).toBe("Vérifier maintenant");
  });

  it("without Internet shows only the last check date, no error and no 'à jour' claim", async () => {
    const { wrapper, bridge } = await mountWith(UpdatePanel, { checkedAgoMs: 3 * 24 * 3_600_000 });
    bridge.setOnline(false);
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.find("[role='alert']").exists()).toBe(false);
    expect(wrapper.get("[data-updates-last]").text()).toBe(
      "Dernière vérification : il y a 3 jours",
    );
    // La dernière tentative a échoué : on ne prétend pas être à jour.
    expect(wrapper.find("[data-updates-status]").exists()).toBe(false);
  });

  it("never checked yet", async () => {
    const { wrapper } = await mountWith(UpdatePanel, { checkedAgoMs: null });
    expect(wrapper.get("[data-updates-last]").text()).toBe(
      "Aucune vérification réussie pour l'instant.",
    );
  });

  it("'Vérifier maintenant' is disabled while checking, with its own label", async () => {
    const { wrapper, store } = await mountWith(UpdatePanel);
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const original = SimulatedUpdateBridge.prototype.check;
    SimulatedUpdateBridge.prototype.check = async function slow(this: SimulatedUpdateBridge) {
      await gate;
      return original.call(this);
    };
    try {
      const pending = store.checkNow();
      await flushPromises();
      const button = wrapper.get("button");
      expect(button.text()).toBe("Vérification en cours…");
      expect(button.attributes("aria-disabled")).toBe("true");
      release();
      await pending;
      await flushPromises();
      expect(wrapper.get("button").text()).toBe("Vérifier maintenant");
    } finally {
      SimulatedUpdateBridge.prototype.check = original;
    }
  });

  it("shows the version found by a manual check, with notes and the update button", async () => {
    const { wrapper, bridge } = await mountWith(UpdatePanel);
    bridge.setFeed(NEWER);
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-updates-status]").text()).toBe("Nouvelle version disponible : 1.1.0");
    expect(wrapper.findAll("button").map((b) => b.text())).toEqual([
      "Vérifier maintenant",
      "Notes de version",
      "Mettre à jour maintenant",
    ]);
    await wrapper.findAll("button")[2]?.trigger("click");
    await flushPromises();
    expect(bridge.calls.install).toBe(1);
  });
});

describe("formatAgo", () => {
  const now = 1_800_000_000_000;
  it.each([
    [30_000, "à l'instant"],
    [5 * 60_000, "il y a 5 min"],
    [60 * 60_000, "il y a 1 heure"],
    [2 * 3_600_000, "il y a 2 heures"],
    [24 * 3_600_000, "il y a 1 jour"],
    [3 * 24 * 3_600_000, "il y a 3 jours"],
  ])("%i ms", (age, text) => {
    expect(formatAgo(now - age, now)).toBe(text);
  });
  it("never goes negative when the clock moved back", () => {
    expect(formatAgo(now + 10_000, now)).toBe("à l'instant");
  });
});
