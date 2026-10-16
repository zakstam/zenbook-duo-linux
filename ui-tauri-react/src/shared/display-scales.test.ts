// Run with: node --test src/shared/display-scales.test.ts (Node strips the types).
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  DISPLAY_SCALE_PRESETS,
  canonicalDisplayScale,
  displayScaleOptions,
  formatDisplayScale,
} from "./display-scales.ts";

const isWhole = (value: number) => Math.abs(value - Math.round(value)) < 1e-9;

test("every preset divides the 2880x1800 Duo panel into whole pixels on the 1/120 grid", () => {
  for (const scale of DISPLAY_SCALE_PRESETS) {
    assert.ok(isWhole(scale * 120), `${scale} is not a multiple of 1/120`);
    assert.ok(isWhole(2880 / scale) && isWhole(1800 / scale), `${scale} gives ${2880 / scale}x${1800 / scale}`);
  }
});

test("labels read as the user picks them", () => {
  assert.deepEqual(
    DISPLAY_SCALE_PRESETS.map(formatDisplayScale),
    ["1.0x", "1.25x", "1.33x", "1.5x", "1.67x", "2.0x"],
  );
});

test("saved and reported scales select their preset instead of adding an option", () => {
  for (const saved of [1.66, 1.67, 1.75, 1.6583333333333334, 1.6666666269302368]) {
    assert.equal(canonicalDisplayScale(saved), 5 / 3, String(saved));
    assert.deepEqual(displayScaleOptions(saved), [...DISPLAY_SCALE_PRESETS]);
  }
  assert.deepEqual(displayScaleOptions(1.1), [1, 1.1, 1.25, 4 / 3, 1.5, 5 / 3, 2]);
});
