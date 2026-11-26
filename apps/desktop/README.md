# Brocode (`apps/desktop`)

- `cargo run -p opencode-desktop -- --live` runs against a local OpenCode server (`http://127.0.0.1:4096` or `~/.config/opencode/service.json`; override with `--server URL` or `OPENCODE_SERVER_URL`).
- For fast local iteration (`scripts/dev-fast.sh` / `pwsh scripts/dev-fast.ps1` with Luau `--plugin-hot`), see [`docs/fast-builds.md`](../../docs/fast-builds.md).
- `apps/desktop/scripts/dev.sh` builds (dev profile with optimized dependencies; `--release` for release), installs `~/Applications/Brocode.app`, and relaunches it with `--live` when the binary changes.
- `cargo run -p opencode-desktop -- --fixture apps/desktop/assets/fixture.json --scene session` runs offline against the bundled fixture.

## Shared parts

Use these; don't draw your own. When one lacks what you need, extend it in its module (and its story), never a local copy beside your feature.

| Need | Use (`opencode_ui::popover`, re-exported as `crate::popover` via `src/shell/menus/popover/`) |
|---|---|
| A menu, picker or popover | `popover`: `surface`, `place`, `scrim`, `search_row`, `List` (group headers), `option` (`Opt`), `nav` for the keys. Looks: `MENU` (actions, short choices) and `LIST` (long lists with search). |
| A popover opening and closing | `popover::place` gives it the ⌘⇧K Popover knobs' entrance and exit; commits that open or close one call `popover::motion::opening` / `closing`. Story: Popover › Open and close. |
| A lit row (pointer or keys) | `popover::lit` (`p.row_active`) |
| A group's title | `popover::group_header` |
| A shortcut at a row's end | `popover::hint` (11 pt at 530, faint) |
