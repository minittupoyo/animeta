# Animeta v0.1 specification

## Purpose and supported environment

Animeta is a Japanese desktop utility that renames anime videos and embeds Annict metadata. Target platforms are Windows, macOS, and Linux. Supported input/output containers are MP4 and MKV; the container and retained encoded streams are not converted. Windows release ZIPs offer bundled and unbundled FFmpeg/ffprobe variants; Linux packages use external tools. OAuth, Annict writes, folder monitoring, sidecars, artwork, transcoding, automatic updates, and signed distribution are outside v0.1. Release packaging produces Windows ZIPs and Linux AppImage/deb packages; see [release documentation](releases.md).

## Workflow

1. Configure and verify an Annict personal access token and media tool paths.
2. Search Annict and select one work. Work search supports cursor pagination; retrieve all episodes, ordered by `sortNumber`.
3. Add videos using file selection, folder selection, or drag and drop. Folder traversal is nonrecursive by default and configurable. Do not follow directory symlinks. Deduplicate canonical paths.
4. Automatically match filename episode numbers to one unique Annict episode, on import and on work selection. Recognize `第01話`, `S01E01`, `EP01`, `E01`, `作品名 - 01`, `01 - 題名`, plain numeric stems, and `[01]`; normalize full-width ASCII/digits. Ignore resolutions and years; conflicting candidates, fractional numbers and ranges remain unresolved. The “自動マッチ” action fills only enabled, unassigned rows without work-level mode or manual overrides. Never replace existing selections on this action. Work changes clear old assignments/overrides and match against the newly retrieved episodes. Do not match by list position or fuzzy titles.
5. Work-level processing supports movies and specials without episode records. Optional per-file manual number, label, and subtitle override the selected episode values without changing Annict links.
6. Choose output directory and operation toggles. Rename and embed are enabled by default. Remove subtitles and remove chapters are independent opt-in toggles, disabled by default. All four operations disabled prevents execution. Copy mode preserves originals; explicit replace mode updates them in their original folder. Removal works without embedding or renaming, requires FFmpeg/ffprobe, and creates verified remuxed output according to the selected mode. Subtitle removal removes every embedded subtitle stream, not burned-in text or external sidecars. Chapter removal removes chapter metadata and recognized MP4 QuickTime chapter data tracks.
7. Preview output names, tags, and validation errors. Unresolved assignments and collisions are excluded until corrected; execute only valid, selected rows.
8. Process sequentially with progress, cancellation, and per-file results. Individual failures do not stop other files. Persist history and allow failed files to be imported again.

## Naming

Default: `{work_title}[ - {episode_label}][ - {episode_title}]` plus the original lowercase container extension. Normal numbered episodes use `第01話`; special `numberText` is retained. Missing episode metadata is allowed in work-level mode.

Variables: `work_title`, `episode_number`, `episode_label`, `episode_title`, `original_stem`, `annict_work_id`, `annict_episode_id`. Numeric variables support `:02` through `:09` padding. Optional nonnested `[segments]` appear only when every referenced value is nonempty. Unknown variables, malformed syntax, and literal path separators are rejected. Metadata-derived prohibited characters become `_`; trim trailing periods/spaces and prefix Windows reserved stems with `_`. Limit a component to 255 UTF-8 bytes and full output paths to 240 UTF-16 units for portable use. Existing paths, including dangling symlinks, and case-insensitive duplicate planned paths are excluded, except the exact selected original path in replace mode. Other videos are never overwritten. No automatic suffixing.

## Metadata mapping

All values are UTF-8. MP4 uses FFmpeg's `use_metadata_tags` (mdta), except when attached cover art requires native MP4/iTunes metadata to preserve `covr`. The mapped fields work in both modes. MKV uses global tags. Keys are read case-insensitively during verification. These tags are portable storage fields, not a promise of display in every player.

| Meaning | MP4 / MKV FFmpeg metadata key | Value |
| --- | --- | --- |
| Video title | `title` | Episode label and subtitle joined with a space; work title if neither is present |
| Work title | `show` | Annict work title |
| Episode display label | `episode_id` | Display label, omitted if absent |
| Numeric episode | `episode_sort` | Integer number, omitted if absent |
| Subtitle | `description` | Subtitle, omitted if absent |
| Annict links | `comment` | Work URL, plus episode URL when an Annict episode is selected |

Preserve the input timestamp origin during remuxing with `-copyts -avoid_negative_ts disabled` so retained chapters and media stay on the same timeline. Copy existing format/stream metadata and chapters; overwrite only mapped fields. Clear mapped episode fields when absent so stale episode information is not carried over. Preserve media streams with `-map 0 -c copy`. For MP4, exclude one unambiguous QuickTime chapter data track (`bin_data` / `text`, one sample per chapter and matching duration) from packet mapping and regenerate it with `-map_chapters 0`. Validate chapter titles and times rather than the regenerated track count or language; subtitles, attachments, timecodes, and other data remain subject to strict stream checks. Verification excludes only explicitly removed subtitle streams and structurally regenerated chapter tracks; all other stream types/codecs, flags and known tags must match. With chapter removal enabled, require no remaining chapters; otherwise require unchanged chapter titles and times. Require no subtitle streams when subtitle removal is enabled. Reject unexpected differences in duration or requested tags. Subtitle/chapter removal and container timestamp normalization can alter total format duration. Check retained track durations within 0.1 seconds, using Matroska DURATION tags when needed. Allow a format-duration difference over 0.5 seconds only when every retained timed track has a known matching duration; otherwise retain the original and report source/output seconds. Arbitrary opaque container elements are not promised to survive remuxing; fixtures cover stream attachments, subtitles, chapter and track metadata.

## File safety and jobs

Canonical input paths are registered in Rust and identified by UUIDs. Snapshots include size and modification time. Validation builds a backend-owned immutable plan. Revalidate fingerprints, writable output directory, operation settings, tool availability, and destination rules before processing. Copy output must differ from every input path. Replace output is restricted to the selected original folder and may equal only that exact original path; other input paths and existing targets remain forbidden. Read-only files and symlinks cannot be replaced.

Use a private temporary file on the output filesystem. Embedding remuxes; rename-only copies bytes into the temporary output. Copy mode publishes with no-clobber semantics. Replace mode atomically replaces the exact original path after verification, or publishes a new filename without clobbering before deleting the old name. If old-name deletion fails or the process stops between publication and deletion, retain both files and report cleanup failure; never remove the original before verified output exists. This does not promise a multi-path filesystem transaction or universal power-loss durability. Cancellation kills/reaps the child process and removes unfinished output; completed output remains. On restart, unfinished jobs become interrupted; clean up only temporary paths recorded by this application. Disk-full and permission errors become per-item failures. If an input changes during processing, do not publish the result.

## Persistence and UI

Settings and history use SQLite in the Tauri app data directory. Tokens use the OS credential store, with an explicitly reported session-only fallback. Tokens never enter logs, settings, history, or frontend state after entry/connection verification. Token deletion clears session and credential storage, reporting deletion failures.

Views: Processing, History, Settings. Use Japanese text, shadcn-vue and Tailwind, bundled Noto Sans JP Variable, Lucide icons, neutral surfaces and a blue accent. Themes: light/dark/system, default system. Settings include a “リネーム方法” selector: copy (default) and replace. Replace mode hides the output-directory control, identifies the consequence in the processing view, and labels execution “置き換えを実行”. History records the mode. Successful replacements refresh registered fingerprints and current filenames without resetting assignments. Initial window 1200×800, minimum 960×640. Accessible labels, visible focus, keyboard alternatives to dropping files, reduced motion, and text explanations for all statuses are required.

UIには操作、選択内容、状態、エラーなど判断に必要な情報を表示します。紹介文や重複する注意書きは常設せず、操作方法・命名ルール・接続や外部ツールの補足は明示的に開くヘルプダイアログにまとめます。ヘルプはキーボードで開閉でき、閉じると起点のボタンへフォーカスを戻します。

## References

- Annict GraphQL: https://developers.annict.com/docs/graphql-api/beta
- Annict source schema: https://github.com/annict/annict/tree/main/rails/app/graphql/beta
- FFmpeg: https://ffmpeg.org/ffmpeg.html
- Container metadata: https://ffmpeg.org/ffmpeg-formats.html
- shadcn-vue: https://www.shadcn-vue.com/docs/installation/vite
