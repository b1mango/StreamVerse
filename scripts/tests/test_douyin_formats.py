from __future__ import annotations

import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "vendor" / "douyin_api"))

from douyin_bridge import (
    build_asset_from_detail,
    collect_formats,
    extract_cover_urls,
    extract_image_urls,
)


def bit_rate(url: str, width: int, height: int, bitrate: int, *, h265: bool = False) -> dict:
    return {
        "bit_rate": bitrate * 1000,
        "is_h265": h265,
        "play_addr": {
            "width": width,
            "height": height,
            "url_list": [url],
        },
    }


class DouyinFormatTests(unittest.TestCase):
    def test_keeps_one_highest_bitrate_item_per_quality_codec_and_container(self) -> None:
        detail = {
            "video": {
                "bit_rate": [
                    bit_rate("https://example.com/1080-low.mp4", 1920, 1080, 1800),
                    bit_rate("https://example.com/1080-high.mp4", 1920, 1080, 3200),
                    bit_rate("https://example.com/1080-h265.mp4", 1920, 1080, 2600, h265=True),
                    bit_rate("https://example.com/720.mp4", 1280, 720, 1400),
                ]
            }
        }

        formats = collect_formats(detail, using_login=True)

        self.assertEqual(
            [(item["label"], item["codec"]) for item in formats],
            [("1080P", "H.264"), ("1080P", "H.265"), ("720P", "H.264")],
        )
        self.assertEqual(formats[0]["bitrateKbps"], 3200)

    def test_prefers_feed_cover_before_original_and_dynamic_variants(self) -> None:
        detail = {
            "video": {
                "dynamic_cover": {"url_list": ["https://example.com/poster.webp"]},
                "cover_original_scale": {
                    "url_list": ["https://example.com/original-scale.jpg"]
                },
                "origin_cover": {"url_list": ["https://example.com/origin.jpg"]},
                "cover": {"url_list": ["https://example.com/video-frame.jpg"]},
            }
        }

        self.assertEqual(extract_cover_urls(detail), [
            "https://example.com/video-frame.jpg",
            "https://example.com/original-scale.jpg",
            "https://example.com/poster.webp",
            "https://example.com/origin.jpg",
        ])

    def test_falls_back_to_dynamic_cover_after_feed_cover(self) -> None:
        detail = {
            "video": {
                "dynamic_cover": {"url_list": ["https://example.com/poster.webp"]},
                "cover": {"url_list": ["https://example.com/video-frame.jpg"]},
            }
        }

        self.assertEqual(extract_cover_urls(detail), [
            "https://example.com/video-frame.jpg",
            "https://example.com/poster.webp",
        ])

    def test_builds_downloadable_asset_for_pure_image_post(self) -> None:
        detail = {
            "aweme_id": "album-1",
            "aweme_type": 68,
            "desc": "测试图册",
            "author": {"nickname": "作者"},
            "images": [
                {
                    "watermark_free_download_url_list": ["https://example.com/1-clean.jpg"],
                    "url_list": ["https://example.com/1-preview.jpg"],
                    "download_url_list": ["https://example.com/1-watermarked.jpg"],
                },
                {
                    "origin_image": {"url_list": ["https://example.com/2-clean.jpg"]},
                    "url_list": ["https://example.com/2-preview.jpg"],
                    "download_url_list": ["https://example.com/2-watermarked.jpg"],
                },
            ],
        }

        asset = build_asset_from_detail(
            detail,
            "https://www.douyin.com/note/album-1",
            using_login=True,
        )

        self.assertIsNotNone(asset)
        self.assertEqual(asset["imageUrls"], [
            "https://example.com/1-clean.jpg",
            "https://example.com/2-clean.jpg",
        ])
        self.assertEqual(asset["formats"], [])

    def test_rejects_known_watermark_only_image_sources(self) -> None:
        detail = {
            "images": [
                {
                    "url_list": [
                        "https://example.com/image~tplv-dy-water-v2:author:1080:1920.webp"
                    ],
                    "download_url_list": ["https://example.com/download-watermarked.jpg"],
                    "owner_watermark_image": {
                        "url_list": ["https://example.com/owner-watermarked.jpg"]
                    },
                }
            ]
        }

        self.assertEqual(extract_image_urls(detail), [])


if __name__ == "__main__":
    unittest.main()
