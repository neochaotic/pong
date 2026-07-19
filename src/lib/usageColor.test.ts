import { describe, expect, it } from "vitest";
import { barColor, isCritical } from "./usageColor";

describe("barColor", () => {
  it("is solid blue anywhere in the 0-60 range", () => {
    expect(barColor(0)).toBe("rgb(94, 106, 210)");
    expect(barColor(30)).toBe("rgb(94, 106, 210)");
    expect(barColor(60)).toBe("rgb(94, 106, 210)");
  });

  it("warms from blue to amber across 60-85, not a hard cutoff", () => {
    const at60 = barColor(60);
    const at72 = barColor(72.5); // midpoint of the 60-85 band
    const at85 = barColor(85);

    expect(at72).not.toBe(at60);
    expect(at72).not.toBe(at85);
    // Roughly halfway between blue (94,106,210) and amber (180,123,16).
    expect(at72).toBe("rgb(137, 115, 113)");
  });

  it("shifts from amber to red across 85-90", () => {
    expect(barColor(85)).toBe("rgb(180, 123, 16)");
    expect(barColor(90)).toBe("rgb(235, 87, 87)");
  });

  it("deepens from red toward 99 without being flat", () => {
    const at90 = barColor(90);
    const at95 = barColor(95);
    const at98 = barColor(98.9);

    expect(at95).not.toBe(at90);
    expect(at98).not.toBe(at95);
  });

  it("is a solid deep red for 99 and 100", () => {
    expect(barColor(99)).toBe(barColor(100));
  });

  it("clamps above 100 the same as 100", () => {
    expect(barColor(120)).toBe(barColor(100));
  });
});

describe("isCritical", () => {
  it("is false below 99", () => {
    expect(isCritical(98.9)).toBe(false);
    expect(isCritical(90)).toBe(false);
  });

  it("is true at 99 and above", () => {
    expect(isCritical(99)).toBe(true);
    expect(isCritical(100)).toBe(true);
  });
});
