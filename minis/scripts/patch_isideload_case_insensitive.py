#!/usr/bin/env python3
"""
Patch vendored isideload so existing App IDs and App Groups on the user's
Apple Developer account are matched case-insensitively and reused with their
exact existing casing (e.g. reusing `cn.oily.gnoix.vx8n6x9t3k` instead of
failing with Apple error 9401 / 35 on `cn.oily.gnoix.VX8N6X9T3K`).
"""
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
ISIDELOAD = ROOT / "SideInstaller" / "rust-core" / "vendor" / "isideload" / "src"


def patch_file(rel: str, replacements: list[tuple[str, str]]) -> None:
    p = ISIDELOAD / rel
    text = p.read_text(encoding="utf-8")
    for old, new in replacements:
        if new in text:
            continue
        if old not in text:
            print(f"ERROR: expected snippet not found in {p}:\n{old}", file=sys.stderr)
            sys.exit(1)
        text = text.replace(old, new, 1)
    p.write_text(text, encoding="utf-8")
    print(f"Patched {p}")


patch_file(
    "dev/app_groups.rs",
    [
        (
            "let matching_group = groups.iter().find(|g| g.identifier == identifier);",
            "let matching_group = groups.iter().find(|g| g.identifier.eq_ignore_ascii_case(identifier));",
        ),
    ],
)

patch_file(
    "sideload/application.rs",
    [
        (
            ".any(|app_id| app_id.identifier == bundle_id)",
            ".any(|app_id| app_id.identifier.eq_ignore_ascii_case(bundle_id))",
        ),
        (
            ".any(|bundle| app_id.identifier == bundle.bundle_identifier().unwrap_or(\"\"))",
            ".any(|bundle| app_id.identifier.eq_ignore_ascii_case(bundle.bundle_identifier().unwrap_or(\"\")))",
        ),
    ],
)

patch_file(
    "sideload/sideloader.rs",
    [
        (
            """            let main_app_id = app_ids
                .iter()
                .find(|app_id| app_id.identifier == main_app_id_str)
                .cloned()
                .ok_or_else(|| {
                    report!(
                        "Main app ID {} not found in registered app IDs",
                        main_app_id_str
                    )
                })?;""",
            """            let main_app_id = app_ids
                .iter()
                .find(|app_id| app_id.identifier.eq_ignore_ascii_case(&main_app_id_str))
                .cloned()
                .ok_or_else(|| {
                    report!(
                        "Main app ID {} not found in registered app IDs",
                        main_app_id_str
                    )
                })?;
            if main_app_id.identifier != main_app_id_str {
                info!(
                    "Reusing existing App ID casing: {} -> {}",
                    main_app_id_str, main_app_id.identifier
                );
                app.update_bundle_id(&main_app_id_str, &main_app_id.identifier)?;
            }
            let group_identifier = app_group
                .as_ref()
                .map(|g| g.identifier.clone())
                .unwrap_or(group_identifier);""",
        ),
    ],
)
