# Animeta development

Read [the product specification](docs/specification.md), [the architecture and verification guide](docs/architecture.md), and [the verification record](docs/verification.md) before making changes.

- Use Vue 3 and TypeScript, shadcn-vue components, Tailwind CSS, and `@fontsource-variable/noto-sans-jp`.
- Use bun for JavaScript dependencies and scripts; commit `bun.lock`. Rust uses Cargo.
- Keep filesystem access, Annict HTTP requests, credential storage, SQLite, and subprocess execution in Rust.
- Default copy mode preserves original videos. Explicit replace mode may replace only the selected original in its own folder after verification. Never overwrite another existing video, even in replace mode. Validate preview and execution; retain the original until verified output is published.
- Never log or persist Annict tokens outside the OS credential store. Fall back to session storage when the credential store is unavailable.
- Use direct process arguments, never a shell, for FFmpeg and ffprobe.
- Keep user-facing text Japanese and keyboard interactions accessible. Support light, dark, and system themes.
- Run `bun run check`, `bun run test`, `bun run build`, and the Rust checks described in the architecture guide for relevant changes.
- Record platform or external-service checks that could not be performed; do not claim they passed.
