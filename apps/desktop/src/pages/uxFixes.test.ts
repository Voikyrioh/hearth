import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "@/App.vue";
import HTag from "@/components/atoms/HTag.vue";
import HTooltip from "@/components/atoms/HTooltip.vue";
import ServicesRightRow from "@/components/molecules/ServicesRightRow.vue";
import AccountDetailDialog from "@/components/organisms/AccountDetailDialog.vue";
import AccountTable from "@/components/organisms/AccountTable.vue";
import type { Account } from "@/link";
import { useServersStore } from "@/stores/servers";
import { startedApp } from "@/test/app";
import { mountContext } from "@/test/mount";
import { tooltipFor } from "@/test/tooltip";

// Revue UX du 2026-10-13 : HRT-134 (bulles), HRT-135 (repère du droit), HRT-139 (Réglages), HRT-141 (clavier).

afterEach(() => {
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

function account(partial: Partial<Account> & { username: string }): Account {
  return {
    id: partial.username.toUpperCase(),
    role: "readonly",
    createdAt: "2026-10-04T10:30:15.250Z",
    lastLoginAt: null,
    sessionsOpen: 0,
    managesServices: false,
    ...partial,
  };
}

/** Une fenêtre de 1000 sur 700, un contrôle placé comme demandé, une bulle de la taille donnée. */
function placed(
  anchor: { left: number; right: number; top: number; bottom: number },
  bubbleSize: { width: number; height: number },
  props: { placement?: "start" | "center" | "end"; side?: "bottom" | "top" } = {},
) {
  Object.defineProperty(document.documentElement, "clientWidth", {
    value: 1000,
    configurable: true,
  });
  Object.defineProperty(document.documentElement, "clientHeight", {
    value: 700,
    configurable: true,
  });
  const wrapper = mount(HTooltip, {
    props: { text: "Une explication", ...props },
    slots: { default: '<button type="button">x</button>' },
    attachTo: document.body,
  });
  vi.spyOn(wrapper.element, "getBoundingClientRect").mockReturnValue({
    ...anchor,
    width: anchor.right - anchor.left,
    height: anchor.bottom - anchor.top,
    x: anchor.left,
    y: anchor.top,
    toJSON: () => ({}),
  });
  const bubble = document.body.querySelector<HTMLElement>('[role="tooltip"]');
  if (!bubble) throw new Error("bulle absente");
  Object.defineProperty(bubble, "offsetWidth", { value: bubbleSize.width, configurable: true });
  Object.defineProperty(bubble, "offsetHeight", { value: bubbleSize.height, configurable: true });
  return { wrapper, bubble };
}
const coord = (bubble: HTMLElement, name: "--tip-x" | "--tip-y") =>
  Number.parseFloat(bubble.style.getPropertyValue(name));

describe("HTooltip hors de son conteneur (HRT-134)", () => {
  it("renders its bubble in the body, not inside a container that would clip it", () => {
    const wrapper = mount(
      {
        components: { HTooltip },
        template:
          '<div class="clip"><HTooltip text="Explication"><button>x</button></HTooltip></div>',
      },
      { attachTo: document.body },
    );
    const bubble = document.body.querySelector('[role="tooltip"]');
    expect(bubble).not.toBeNull();
    expect(wrapper.element.contains(bubble)).toBe(false);
    expect(bubble?.parentElement).toBe(document.body);
    wrapper.unmount();
  });

  it("keeps the bubble inside the window for a control stuck to the right edge", async () => {
    const { wrapper, bubble } = placed(
      { left: 960, right: 1000, top: 100, bottom: 130 },
      { width: 260, height: 60 },
    );
    await wrapper.trigger("mouseenter");
    await flushPromises();
    expect(coord(bubble, "--tip-x") + 260).toBeLessThanOrEqual(1000);
    expect(coord(bubble, "--tip-x")).toBeGreaterThanOrEqual(0);
    expect(coord(bubble, "--tip-y")).toBeGreaterThan(130);
    wrapper.unmount();
  });

  it("keeps the bubble inside the window for a control stuck to the left edge", async () => {
    const { wrapper, bubble } = placed(
      { left: 0, right: 40, top: 100, bottom: 130 },
      { width: 260, height: 60 },
    );
    await wrapper.trigger("mouseenter");
    await flushPromises();
    expect(coord(bubble, "--tip-x")).toBeGreaterThanOrEqual(0);
    wrapper.unmount();
  });

  it("moves above the control when there is no room below, and closes on Escape", async () => {
    const { wrapper, bubble } = placed(
      { left: 100, right: 140, top: 650, bottom: 680 },
      { width: 200, height: 60 },
    );
    await wrapper.get("button").trigger("focusin");
    await flushPromises();
    expect(coord(bubble, "--tip-y")).toBeLessThan(650);
    await wrapper.trigger("keydown", { key: "Escape" });
    expect(bubble.style.display).toBe("none");
    wrapper.unmount();
  });
});

describe("HTag interactif (HRT-135)", () => {
  it("is a real button that can be reached and activated with the keyboard", async () => {
    const wrapper = mount(HTag, { props: { interactive: true }, slots: { default: "Peut gérer" } });
    expect(wrapper.element.tagName).toBe("BUTTON");
    expect(wrapper.attributes("type")).toBe("button");
    await wrapper.trigger("click");
    expect(wrapper.emitted("click")).toHaveLength(1);
    expect(mount(HTag, { slots: { default: "x" } }).element.tagName).toBe("SPAN");
  });
});

describe("tableau des comptes : repère du droit et bulles (HRT-134, HRT-135)", () => {
  const mountTable = async (accounts: Account[], meId = "MARIE") => {
    const ctx = await startedApp();
    useServersStore().setCurrent("forge");
    return mount(AccountTable, {
      props: { accounts, meId, rightsKnown: true },
      global: { plugins: [ctx.pinia] },
      attachTo: document.body,
    });
  };

  it("explains the right where it is read, and opens the detail on click", async () => {
    const wrapper = await mountTable([
      account({ username: "marie", role: "admin", managesServices: true }),
    ]);
    const tag = wrapper.get("[data-services-right]");
    expect(tag.element.tagName).toBe("BUTTON");
    const tip = tooltipFor(tag).text();
    expect(tip).toContain("démarrer, arrêter, redémarrer et déployer des services");
    expect(tip).toContain("Clique pour voir comment ce droit se donne");
    await tag.trigger("click");
    expect(wrapper.emitted("detail")?.[0]?.[0]).toMatchObject({ username: "marie" });
    wrapper.unmount();
  });

  it("explains an inactive « Fermer les sessions » for an account without any session", async () => {
    const wrapper = await mountTable([
      account({ username: "marie", role: "admin" }),
      account({ username: "paul", sessionsOpen: 0 }),
    ]);
    const close = wrapper
      .get('[data-account="paul"]')
      .get('button[aria-label^="Fermer les sessions"]');
    expect(close.attributes("aria-disabled")).toBe("true");
    expect(tooltipFor(close).text()).toBe("Ce compte n'a aucune session ouverte.");
    wrapper.unmount();
  });

  it("explains the refused removal with a bubble that lives outside of the table", async () => {
    const wrapper = await mountTable([
      account({ username: "marie", role: "admin" }),
      account({ username: "paul", managesServices: true }),
    ]);
    const remove = wrapper.get('[data-account="paul"]').get('button[aria-label^="Supprimer"]');
    const bubble = tooltipFor(remove);
    expect(bubble.text()).toContain("Seul un compte qui le peut aussi");
    expect(wrapper.element.contains(bubble.element)).toBe(false);
    wrapper.unmount();
  });
});

describe("détail du compte (HRT-134, HRT-135)", () => {
  it("starts with what the right allows, and writes the removal reason without any hover", () => {
    HTMLDialogElement.prototype.showModal = function showModal() {
      this.setAttribute("open", "");
    };
    const wrapper = mount(AccountDetailDialog, {
      props: {
        rightsKnown: true,
        account: account({ username: "paul", managesServices: true }),
        meId: "MARIE",
        removalReason: "Ce compte peut gérer les services : seul un titulaire peut le supprimer.",
      },
      attachTo: document.body,
    });
    const text = document.body.textContent ?? "";
    expect(text).toContain(
      "Ce droit permet de démarrer, arrêter, redémarrer et déployer des services",
    );
    expect(document.querySelector("[data-removal-reason]")?.textContent).toContain(
      "seul un titulaire",
    );
    wrapper.unmount();
  });
});

describe("ligne du droit (HRT-135)", () => {
  it("warns before the command that removes your OWN right, and only then", () => {
    const own = mount(ServicesRightRow, {
      props: { username: "marie", manages: true, known: true, own: true },
    });
    expect(own.get("[data-rights-own-warning]").text()).toContain("C'est ton propre compte");
    expect(own.get("pre").text()).toBe("sudo hearth-agent account services marie --revoke");
    const other = mount(ServicesRightRow, {
      props: { username: "paul", manages: true, known: true, own: false },
    });
    expect(other.find("[data-rights-own-warning]").exists()).toBe(false);
    const grant = mount(ServicesRightRow, {
      props: { username: "marie", manages: false, known: true, own: true },
    });
    expect(grant.find("[data-rights-own-warning]").exists()).toBe(false);
  });
});

async function boot(path: string) {
  const ctx = await mountContext();
  await ctx.router.push(path);
  await ctx.router.isReady();
  const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  return { ...ctx, wrapper };
}

describe("Réglages dit une chose vraie par carte (HRT-139)", () => {
  it("names the PC card, the server card with its versions, explains « agent » once, and puts « Mon compte » in a card", async () => {
    const { wrapper } = await boot("/settings");
    expect(wrapper.get("[data-updates-status]").text()).toContain("sur ce PC");
    expect(wrapper.findAll("[data-agent-explain]")).toHaveLength(1);
    expect(wrapper.get("[data-agent-explain]").text()).toBe(
      "L'agent est le programme Hearth installé sur ton serveur.",
    );
    const forge = wrapper.get('[data-agent-card][data-server="forge"]');
    expect(forge.get("[data-agent-versions]").text()).toMatch(
      /^Version installée sur ce serveur : \d+\.\d+\.\d+\./,
    );
    const own = wrapper.get("[data-own-accounts]");
    expect(own.classes()).toContain("settings__panel");
    expect(own.findAll(".mine")).toHaveLength(2);
    wrapper.unmount();
  });
});

describe("clavier (HRT-141)", () => {
  it("has a skip link as the first stop, which moves the focus to the main area", async () => {
    const { wrapper } = await boot("/settings");
    const first = wrapper.element.querySelector("button, a, input, select, [tabindex]");
    expect(first?.hasAttribute("data-skip-link")).toBe(true);
    expect(first?.textContent?.trim()).toBe("Aller au contenu");
    await wrapper.get("[data-skip-link]").trigger("click");
    expect(document.activeElement?.tagName).toBe("MAIN");
    wrapper.unmount();
  });

  it("declares the content of a server page as one named main area", async () => {
    const { wrapper } = await boot("/servers/forge/accounts");
    expect(wrapper.findAll("main")).toHaveLength(1);
    expect(wrapper.get("main").attributes("aria-label")).toBeTruthy();
    wrapper.unmount();
  });
});
