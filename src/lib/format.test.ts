import { describe, it, expect, vi, afterEach } from "vitest";

import {
  deterministicHue,
  formatBytes,
  formatPercent,
  formatSpeed,
  monogram,
  shortRelative,
} from "./format";

describe("formatBytes", () => {
  it("returns em-dash for negative or non-finite input", () => {
    expect(formatBytes(-1)).toBe("—");
    expect(formatBytes(Number.NaN)).toBe("—");
    expect(formatBytes(Number.POSITIVE_INFINITY)).toBe("—");
  });

  it("returns whole bytes without a decimal under 1 KB", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1023)).toBe("1023 B");
  });

  it("steps up to KB / MB / GB and prints one decimal when below 100 of that unit", () => {
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(1024 * 1024)).toBe("1.0 MB");
    expect(formatBytes(1024 * 1024 * 1024)).toBe("1.0 GB");
  });

  it("uses no decimals for triple-digit values within a unit", () => {
    // 999 KB ≈ 999.0 → less than 100 stays one decimal, ≥100 drops it
    expect(formatBytes(150 * 1024)).toBe("150 KB");
    expect(formatBytes(999 * 1024)).toBe("999 KB");
  });
});

describe("formatPercent", () => {
  it("returns 0% when total is zero or negative", () => {
    expect(formatPercent(50, 0)).toBe("0%");
    expect(formatPercent(50, -10)).toBe("0%");
  });

  it("clamps to 100% when over-counting", () => {
    expect(formatPercent(150, 100)).toBe("100%");
  });

  it("rounds to the nearest integer percent", () => {
    expect(formatPercent(1, 3)).toBe("33%");
    expect(formatPercent(2, 3)).toBe("67%");
  });
});

describe("formatSpeed", () => {
  it("appends '/s' to the formatted byte value", () => {
    expect(formatSpeed(1024)).toBe("1.0 KB/s");
    expect(formatSpeed(0)).toBe("0 B/s");
  });
});

describe("monogram", () => {
  it("returns the first two letters uppercased for single-word names", () => {
    expect(monogram("Alice")).toBe("AL");
    expect(monogram("X")).toBe("X");
  });

  it("uses initials for multi-word names", () => {
    expect(monogram("Alice MacBook")).toBe("AM");
    expect(monogram("john doe smith")).toBe("JD");
  });

  it("falls back when input is empty / whitespace", () => {
    expect(monogram("")).toBe("?");
    expect(monogram("   ")).toBe("?");
    expect(monogram("", "X")).toBe("X");
  });

  it("trims surrounding whitespace before slicing", () => {
    expect(monogram("  bob  ")).toBe("BO");
  });
});

describe("deterministicHue", () => {
  it("returns the same hue for the same seed", () => {
    expect(deterministicHue("peer-1")).toBe(deterministicHue("peer-1"));
  });

  it("returns a value in [0, 360)", () => {
    for (const seed of ["a", "abcd", "peer-uuid-here", ""]) {
      const h = deterministicHue(seed);
      expect(h).toBeGreaterThanOrEqual(0);
      expect(h).toBeLessThan(360);
    }
  });

  it("usually distinguishes different seeds", () => {
    // Not a guarantee, but a regression catch: at least some of these
    // hand-picked seeds should land on different hues.
    const hues = new Set(
      ["alpha", "bravo", "charlie", "delta", "echo"].map(deterministicHue),
    );
    expect(hues.size).toBeGreaterThanOrEqual(2);
  });
});

describe("shortRelative", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns empty string for null / undefined / non-parseable input", () => {
    expect(shortRelative(undefined)).toBe("");
    expect(shortRelative(null)).toBe("");
    expect(shortRelative("not-a-date")).toBe("");
  });

  it("buckets timestamps into 'just now' / minutes / hours / days", () => {
    const now = new Date("2026-05-16T12:00:00Z").getTime();
    vi.useFakeTimers();
    vi.setSystemTime(now);

    expect(shortRelative(new Date(now - 30_000).toISOString())).toBe("just now");
    expect(shortRelative(new Date(now - 5 * 60_000).toISOString())).toBe("5m ago");
    expect(shortRelative(new Date(now - 3 * 3_600_000).toISOString())).toBe("3h ago");
    expect(shortRelative(new Date(now - 2 * 86_400_000).toISOString())).toBe("2d ago");
  });
});
