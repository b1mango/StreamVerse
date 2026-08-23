import assert from "node:assert/strict";
import test from "node:test";

import { visibleFormats } from "../../src/lib/media.ts";


function format(id, label, resolution, codec, bitrateKbps, requiresLogin = false) {
  return {
    id,
    label,
    resolution,
    bitrateKbps,
    codec,
    container: "MP4",
    noWatermark: true,
    requiresLogin,
    requiresProcessing: false,
    recommended: id === "1080-h264-high",
    directUrl: `https://example.com/${id}.mp4`
  };
}


test("visible formats keep one highest-bitrate row per quality, codec, and container", () => {
  const asset = {
    formats: [
      format("1080-h264-public", "1080P", "1920x1080", "H.264", 1800),
      format("1080-h264-high", "1080P", "1920x1080", "H.264", 3200, true),
      format("1080-h265-low", "1080P", "1080x1920", "H.265", 2100),
      format("1080-h265-high", "1080P", "1080x1920", "H.265", 2800, true),
      format("720-h264", "720P", "720x1280", "H.264", 1400)
    ]
  };

  const formats = visibleFormats(asset, "active");

  assert.deepEqual(
    formats.map((item) => [item.label, item.codec, item.bitrateKbps]),
    [
      ["1080P", "H.264", 3200],
      ["1080P", "H.265", 2800],
      ["720P", "H.264", 1400]
    ]
  );
});


test("guest format filtering happens before deduplication", () => {
  const asset = {
    formats: [
      format("1080-h264-public", "1080P", "1920x1080", "H.264", 1800),
      format("1080-h264-high", "1080P", "1920x1080", "H.264", 3200, true)
    ]
  };

  assert.equal(visibleFormats(asset, "guest")[0].id, "1080-h264-public");
});
