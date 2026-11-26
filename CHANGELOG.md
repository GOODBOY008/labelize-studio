# Changelog

## [v0.6.0] — 2026-11-26

- Added reusable printer templates for ZPL, TSPL/TSC, EPL2, CPCL, and DPL. The shipping-label examples show how to fill a template with Labelize 1.13.0 through the CLI, HTTP API, or Rust SDK.
- Added parameter previews for unsaved designs and exported `.tpl.*` files. Each tab accepts its own record of string values. The same order number can appear as text, Code 128, and QR, and the PNG or PDF export uses the filled printer bytes.
- Added fixed-layout ZPL export at 8 and 12 dots/mm. Opening a BarTender `.btw` design now shows how to export a printable file from BarTender.
- Studio now follows the system language on first launch. The English and Simplified Chinese language packs include field validation messages.

## [v0.5.0] — 2026-11-12

- Added per-document vector PDF export for designs and printer files. Supported text stays searchable, shapes and barcodes retain their geometry, and embedded bitmaps keep their original detail.
- Added optional antialiasing for previews and raster output. PNG exports from a vector scene can use 1×–4× resolution while keeping the label's physical dimensions.
- Switched printer-language rendering to the published Labelize 1.12.0 engine and refreshed the application and installer icons.

## [v0.4.0] — 2026-11-10

- Renamed **Labelize Printer** to **Labelize Studio**. Upgrades preserve settings, drafts, job history, and the existing **Labelize Printer** system queue.
- Expanded the designer to 15 object types, including ten barcode formats. Added text wrapping, rotation, image fitting, fills, rounded corners, and controls to name, hide, lock, and reorder layers.
- Reused decoded previews when switching tabs so labels appear faster.
- Added in-app checks for stable releases, cancellable downloads, and SHA-256 verification. Studio saves workspace drafts and checks that the print queue is idle before handing off to the installer. Homebrew installations upgrade through Homebrew.

## [v0.3.0] — 2026-11-09

- Added a multi-tab workspace with independent edits, design undo history, page selection, zoom, and export settings. Source and design edits refresh the preview automatically.
- Added local draft autosave and session recovery. Reopening Studio restores unfinished edits, and saving warns when the source file has changed elsewhere.
- Added PDF and PNG export from each tab. Accepted exports continue in the background after the tab or window closes.
- Added recent files, job filtering, and a compact layout that moves the inspector into a drawer on narrow windows.

## [v0.2.0] — 2026-11-08

- Added a workspace for ZPL, EPL, TSPL/TSC, CPCL, DPL, and PRN files with language detection, editable UTF-8 source, and page previews. Binary files open read-only to preserve their bytes.
- Introduced the visual designer with text, rectangles, images, Code 128, and QR codes. Designs save as portable `.labelize.json` files with embedded images and an optional imported font.
- Added multipage raster PDF and per-page PNG export for label files and designs through the background job queue. Recent jobs now show the source format.

## [v0.1.2] — 2026-11-07

- Added English and Simplified Chinese interface switching. The desktop app remembers the selection after restart and keeps unsaved settings when switching languages.
- Updated the desktop layout for smaller windows, added the macOS Dock icon to packaging checks, and corrected the Zebra driver link in Windows setup.

## [v0.1.1] — 2026-11-07 (preview)

- Published preview installers for Windows x64, macOS Apple Silicon and Intel, and Debian/Ubuntu amd64 with a SHA-256 checksum manifest.
- The local virtual printer captured RAW ZPL and CUPS raster jobs as multipage PDF or per-page PNG while the desktop window was closed. The job history let users open saved files and retry failed jobs.
- The IPP adapter converted sRGB pages from CUPS to grayscale while preserving their size and order.
- Bundled the Windows runtime through static linking and updated the installer to stop the existing background service during upgrades.

[v0.6.0]: https://github.com/GOODBOY008/labelize-studio/releases/tag/v0.6.0
[v0.5.0]: https://github.com/GOODBOY008/labelize-studio/releases/tag/v0.5.0
[v0.4.0]: https://github.com/GOODBOY008/labelize-studio/releases/tag/v0.4.0
[v0.3.0]: https://github.com/GOODBOY008/labelize-studio/releases/tag/v0.3.0
[v0.2.0]: https://github.com/GOODBOY008/labelize-studio/releases/tag/v0.2.0
[v0.1.2]: https://github.com/GOODBOY008/labelize-studio/releases/tag/v0.1.2
[v0.1.1]: https://github.com/GOODBOY008/labelize-studio/releases/tag/v0.1.1
