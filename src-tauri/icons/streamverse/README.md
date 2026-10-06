# StreamVerse Icons

This directory is the canonical icon source for StreamVerse packaging.

- `source.png`: source PNG used for generated icons.
- `icon.icns`: macOS app and DMG icon.
- `icon.ico`: Windows app icon.
- `installer-icon.ico`: Windows installer icon.

`src-tauri/tauri.conf.json` points to this directory for bundle icons.

All icons use the same transparent artwork as `../icon.png` in the app UI.

On macOS Tahoe, Finder adds a light tile to irregular legacy app icons.
`npm run tauri:build` runs `scripts/finalize-macos-icons.mjs` after packaging
to apply the UI artwork as a Finder custom icon to the app and DMG. The DMG
retains the resource fork and FinderInfo metadata when the app is dragged out.
Run that script again after invoking the Tauri CLI directly (pass a custom
bundle directory as its first argument for non-default targets).

The custom icon is applied after signing. Normal signature verification still
works; strict verification rejects Finder custom icon metadata. A future
Developer ID/notarized release must revalidate this packaging approach.
