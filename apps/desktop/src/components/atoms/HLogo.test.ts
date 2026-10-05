import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import HIcon from "./HIcon.vue";
import HLogo from "./HLogo.vue";

describe("HLogo", () => {
  it("is announced as Hearth unless decorative", () => {
    expect(mount(HLogo).attributes("aria-label")).toBe("Hearth");
    const decorative = mount(HLogo, { props: { decorative: true } });
    expect(decorative.attributes("aria-hidden")).toBe("true");
    expect(decorative.attributes("aria-label")).toBeUndefined();
  });

  it("uses the simplified drawing at small size", () => {
    const small = mount(HLogo, { props: { size: "sm" } });
    const large = mount(HLogo, { props: { size: "lg" } });
    expect(small.findAll("path")[1]?.attributes("fill-rule")).toBeUndefined();
    expect(large.findAll("path")[1]?.attributes("fill-rule")).toBe("evenodd");
  });
});

describe("HIcon", () => {
  it("is hidden from assistive tech without a label", () => {
    expect(mount(HIcon, { props: { name: "plus" } }).attributes("aria-hidden")).toBe("true");
  });

  it("is an image when labelled", () => {
    const icon = mount(HIcon, { props: { name: "settings", label: "Réglages" } });
    expect(icon.attributes("role")).toBe("img");
    expect(icon.attributes("aria-label")).toBe("Réglages");
  });
});
