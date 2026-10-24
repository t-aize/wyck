@AGENTS.md

## Claude Code

- Prefer the dedicated file tools over shell commands. On Windows the shell may be PowerShell or Git Bash.
- After code changes, run `bun run check:fix` and `bun run typecheck` before reporting the task as done.
- Keep this file short. Shared guidance belongs in `AGENTS.md`; put only Claude-specific notes here.
- Personal preferences go in `CLAUDE.local.md` (gitignored), not here.
