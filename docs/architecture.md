# Architecture and verification

## Boundaries

Vue/Pinia owns transient UI state and typed IPC calls. Rust owns registered input paths, immutable validated plans, token/session state, Annict requests, external tools, execution/cancellation, and SQLite. The frontend never executes a shell or accesses arbitrary filesystem APIs. The dialog plugin provides native pickers; Tauri drag/drop supplies paths to the registration command.

Rust modules: `models` (IPC DTOs), `naming` (templates/inference), `annict` (GraphQL), `storage` (settings/history), `media` (probe/remux/copy), `lib` (Tauri orchestration). Media transactions include disposable output tracking and no-clobber publication; production jobs and fixture tests use the same transaction. IPC uses camelCase fields and Japanese error strings. No public network server is exposed.

## IPC contracts

- `load_settings`, `save_settings`: typed Settings; tool paths, output directory, outputMode (copy/replace), template, recursive import, operation toggles (rename/embed/removeSubtitles/removeChapters), theme.
- `auth_status`, `connect_annict`, `delete_token`: credential status and session/persistent mode; never return the token.
- `search_works(query, after)`: page of Work objects plus cursor/hasNextPage.
- `get_episodes(workId)`: full Episode list, fetched by cursor and sorted.
- `import_paths(paths, recursive)`: registered video objects and import warnings.
- `check_tools(settings)`: FFmpeg/ffprobe versions and per-tool failures.
- `validate_plan(request)`: work + file assignments + settings → backend plan ID and preview rows. Invalid rows have explanations and no executable item.
- `start_job(planId)`, `cancel_job`, `get_job`: only one job at a time; validated plans are single-use.
- `list_history`, `clear_history`: persisted job snapshots. Clear is available only when no job is running.
- `job-update` event: full JobSnapshot including job ID, item IDs, statuses, progress and results. `get_job` supports reload/recovery when an event was missed.

### Key DTOs

Work: Annict ID, title, media, season year/name, episode count, no-episodes flag. Episode: Annict ID, nullable number/numberText/title, sort number. ImportedFile: UUID, canonical path, display name, byte size, modification timestamp, guessed number, probe error. Assignment: file ID, enabled, episode ID or work-level mode, optional manual number/label/subtitle. Preview: input ID/name, output path/name, resolved tags, warnings/errors. JobSnapshot: ID, start time, status, current item/progress, result list, total count. JobSnapshot also stores outputMode and updatedFiles (defaulting to copy/empty for old history). After a successful replacement, refresh the registered file and emit its new path/fingerprint for the processing list. History stores complete snapshots and original resolved metadata for review; retry imports failed input paths and requires a new preview.

## Episode matching

Rust infers one episode number from explicit markers or delimited release filenames. Vue assigns only a unique matching Annict number after importing files or selecting a work. The manual rematch action preserves existing assignments, work-level choices, disabled rows, and manual overrides; changes invalidate the preview. No list-position or fuzzy-title guessing is used.

## Persistence and execution

SQLite contains versioned settings and JSON job snapshots; OS credential storage is independent. HTTP has timeouts and at most two bounded retries for transient/429 errors (respect a bounded Retry-After). GraphQL errors and invalid tokens are not silently converted into empty results. Search pagination is user-driven; episode pagination is automatic and rejects repeated cursors.

Direct FFmpeg/ffprobe arguments prevent shell injection. Remux with `-copyts -avoid_negative_ts disabled` to retain the input timeline and chapter positions across FFmpeg versions, including MKV files with nonzero start times. Tool failures and stderr are bounded and exposed as actionable item errors. Serialize imports/probing off the UI thread. Jobs execute sequentially; progress comes from FFmpeg's `-progress` pipe and byte-copy progress. MP4 QuickTime chapter data is regenerated from chapter metadata instead of copied twice. Only a unique `data/bin_data/text` track with matching chapter sample count and duration is classified this way; verify chapter tags and times, and keep strict checks for other streams. Stream-count errors include both stream inventories. Use tempfile's no-clobber publication on the destination filesystem in copy mode. Replace mode permits only the exact selected original or a free new filename in the original folder. Recheck fingerprints and destinations before commit. Same-path replacement uses `persist`; renamed replacement publishes with `persist_noclobber` then rechecks and removes the original. On Unix, sync the output directory before removing the old name. A crash between publication and deletion leaves both files; no startup recovery deletes the old video. Preserve source permissions in replace mode. Persist snapshots at state transitions and throttle progress events. Record temporary output paths for startup cleanup.

Settings default both removal flags to false, including deserialization of older saved settings. `requires_remux` gates tool checks, probing, and processing for embedding or either removal action. Negative stream mapping removes embedded subtitles; `-map_chapters -1` prevents chapter copying and recognized QuickTime chapter data is also excluded. Reindex output dispositions after all excluded tracks. Verification checks that requested targets are absent while verifying every retained track. Compare known retained track playback spans within 0.1 seconds (MP4 stream duration, or Matroska DURATION end timestamp minus start). A container-duration difference over 0.5 seconds is allowed only when all retained timed tracks have known matching spans; attachment/cover-art streams do not define playback. Otherwise reject before publication with both measured durations. Preserve MP4 cover art using native metadata when `attached_pic` is present.

## Development and checks

```sh
bun install
bun run tauri dev
bun run check
bun run test
bun run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Media integration tests need FFmpeg/ffprobe on PATH (or `ANIMETA_TEST_FFMPEG` and `ANIMETA_TEST_FFPROBE`). They generate small disposable fixtures, verify embedded tags and stream preservation, compare original bytes, and check cancellation/failure/no-clobber behavior. UI tests use mocked IPC, not a real Annict account. Browser preview offers an explicitly labeled sample workflow and never claims to write local files.

Acceptance: successfully process Japanese-path MP4/MKV with original bytes unchanged; tags read back; reject unresolved/duplicate outputs; persist settings/history; retry failed rows; cancel without final partial output; recover interrupted history on restart. Check keyboard interaction and both themes. Build/run smoke tests on Windows/macOS/Linux are required for release, and any unavailable platform or live Annict verification must be stated explicitly.

## Repository automation

The default branch is `main`. Commit Bun and Cargo lockfiles; local app data, credentials, media, build outputs, and test reports are ignored. `.gitattributes` keeps text files in LF and `.editorconfig` aligns editor settings. Project code uses MIT; retained third-party notices are linked from the README.

`.github/workflows/ci.yml` runs frontend checks/build/UI tests and Rust format/Clippy/tests on pushes to main, pull requests, and manual runs. Rust CI installs FFmpeg so media tests run rather than skip. `.github/workflows/windows.yml` is manual: build with cargo-xwin for x86_64-pc-windows-msvc and retain the executable as a 14-day artifact. Neither workflow publishes a GitHub release. Workflow actions are pinned to commit SHAs and use read-only repository permissions.

## Latest verification

See [the verification record](verification.md) for completed checks and platform/service limitations.
