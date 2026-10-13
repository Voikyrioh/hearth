import { describe, expect, it } from "vitest";
import { diskTitle, probeTitle } from "./labels";

describe("noms lisibles du tableau de bord (HRT-142)", () => {
  it("names the root disk « Disque système » and any other disk by its folder, device name in second place", () => {
    expect(diskTitle("/dev/nvme0n1p2", "/")).toEqual({
      title: "Disque système",
      technical: "/dev/nvme0n1p2",
    });
    expect(diskTitle("/dev/sda1", "/mnt/data")).toEqual({
      title: "/mnt/data",
      technical: "/dev/sda1",
    });
  });

  it("keeps the name when the disk has no separate folder", () => {
    expect(diskTitle("C:", "C:")).toEqual({ title: "C:", technical: null });
  });

  it("gives a simple name to the known probes only", () => {
    expect(probeTitle("coretemp Package id 0")).toEqual({
      title: "Processeur",
      technical: "coretemp Package id 0",
    });
    expect(probeTitle("nvme Composite")).toEqual({
      title: "Disque SSD",
      technical: "nvme Composite",
    });
    expect(probeTitle("acpitz temp1")).toEqual({ title: "acpitz temp1", technical: null });
  });
});
