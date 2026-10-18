# Rebase log

One record per upstream sync, newest first. Written by `.github/workflows/sync-upstream.yml`; see `FORK-NOTES.md` for how the fork is kept.

## 2026-10-18 22:22 UTC · upstream `e5b12ad` → `5a0b63f`

Pull request: https://github.com/Maxcimkj/slot-gb-custom/pull/2
Run: https://github.com/Maxcimkj/slot-gb-custom/actions/runs/39170914840

### Upstream changes (10 commits)

- `5a0b63f` Add remappable Turbo A and Turbo B rows, #127 (Brandon T. Kowalski)
- `1b20432` Mark RG34XXSP stick support for verification and document label sizes (Brandon T. Kowalski)
- `2c82294` Make controls global with per-page Reset All and add a lid power off timer (Brandon T. Kowalski)
- `bfb8bb2` Steer with the left stick on Anbernics that have one, #106 #107 (Brandon T. Kowalski)
- `a1c3314` Open the core off the frame loop and light the screen while a ROM loads, #119 (Brandon T. Kowalski)
- `ad5ce8a` Unload WiFi before power off and add a lid watcher, #117 (Brandon T. Kowalski)
- `e1bfb2b` Remap console buttons, shortcuts and shelf keys from the menu (Brandon T. Kowalski)
- `6bc023d` Add task deploy:all to push to every attached device (Brandon T. Kowalski)
- `f87b5a0` Repair a read-only SD card from SELECT + START (#72) (Brandon T. Kowalski)
- `b0fe199` Pull run-ahead and keep mGBA's flash bank through a state load, closes #104 (Brandon T. Kowalski)

### Fork changes kept on top (7 commits)

- `9162c8b` Merge the sync pull request when every required check passes
- `5cd26fb` Say which token the sync run uses
- `9a05932` Name the sync workflow's token secret SYNC_TOKEN
- `9f2feda` Keep test sync branches under sync/ so they do not clash with the base branch
- `4f436d4` Name the sync workflow's token secret SYNC_AI_TOKEN
- `de2d96a` Sync with upstream through a pull request resolved by a coding agent
- `0b46d92` Add new fit mode for gb, gbc and bezels

### Conflicts

- Resolved by OpenCode with deepseek/deepseek-v4.1-flash: `crates/slot/src/app.rs`, `crates/slot/src/frontend.rs`

#### Agent notes

##### crates/slot/src/app.rs
Upstream added a `!self.bound_to_game(Btn::L1)` / `!self.bound_to_game(Btn::R1)` clause to the `slot_owns_the_shoulders()` guard in `Phase::Playing` and reformatted each arm onto two lines. The fork's change is the target of those arms: L1 goes to `self.video_mode.bigger()` and R1 to `self.video_mode.smaller()`, so the shoulder steps through Actual, Fit and Stretch instead of jumping straight to Stretch/Actual. I kept upstream's guard and formatting verbatim and only swapped in the fork's `bigger()`/`smaller()` calls, so neither side's work was dropped. The other fork additions in this file (`fits_picture()`, `softens_picture()` and their neighbours) auto-merged around upstream's new `taken_buttons()`/`bound_to_game()` code.

##### crates/slot/src/frontend.rs
Upstream rewrapped the `slot_ui` import block and added several new names (`menu_detail_face`, `prompt_legends`, `quick_description_face`, `quick_label_face`, `repair_legends`). The fork inserted `fit_game_bezel_face` into that same list for the Fit bezel art. I took upstream's import list as the base and added `fit_game_bezel_face` in alphabetical order after `default_game_bezel_face`, then let `cargo fmt --all` rewrap the block. The fork's other changes in this file (`set_soft_picture` after `set_game_source_rect`, the `(pick, fits)` match in `sync_game_bezel`, the `bool` in the `BezelSync` cache key) auto-merged cleanly and were left as they landed.

##### crates/slot-gfx/tests/pipeline.rs
Upstream did not touch this file; the fork appended two Fit-shader tests (`the_fitted_picture_fills_the_height_with_bars_only_at_the_sides` and `softening_gives_every_fitted_game_boy_row_the_same_weight`). The file imports `HeadlessSurface`, which is `#[cfg(target_os = "macos")]`, but unlike every sibling render test (`layout.rs`, the `slot` and `slot-ui` render tests) it had no `#![cfg(target_os = "macos")]`, so `cargo clippy --workspace --all-targets` failed to compile it on the Linux runner. I added the same `#![cfg(target_os = "macos")]` gate the fork documents for renderer tests, which skips the file off macOS and still runs the Fit tests on a Mac.

##### crates/slot/examples/record.rs
Upstream's `record` example drives `HeadlessSurface`, the macOS-only core-profile context, and the fork does not touch it. Because `cargo clippy --workspace --all-targets` builds examples, this unmodified file broke the Linux host check as well. I gated the `HeadlessSurface` import and the existing `main` behind `#[cfg(target_os = "macos")]` and added a non-macOS `main` that prints one line and exits, with a `cfg_attr` that allows the now-dead helpers off macOS. The example records exactly as before on macOS and compiles everywhere else.

##### Checks
- `cargo fmt --all` and `cargo fmt --all -- --check`: pass.
- `cargo clippy --workspace --all-targets -- -D warnings`: pass after the pipeline.rs and record.rs gates above.
- `cargo clippy -p slot --no-default-features --features device -- -D warnings`: pass.
- `cargo build --profile device --target-dir target-device -p slot --no-default-features --features device`: pass.
- `cargo test -p slot-gfx`: pass (the `pipeline.rs` and `layout.rs` render tests report 0 tests on this Linux host because they are macOS-gated).
- `cargo test -p slot --no-fail-fast`: pass, including the fork's `l_and_r_change_the_mode_on_a_game_boy_cart_and_not_on_a_gba_one`, `the_picture_mode_is_remembered_per_cart`, `only_the_fitted_picture_is_softened`, `the_fitted_picture_keeps_its_bezel_and_the_stretched_one_drops_it` and `a_gba_cart_saved_as_fit_still_shows_the_whole_picture`. The macOS-gated `render_*` tests report 0 tests here; they cannot be verified on this machine.

### Checks

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | ✅ success |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ success |
| `cargo clippy -p slot --no-default-features --features device -- -D warnings` | ✅ success |
| `cargo build --profile device -p slot --no-default-features --features device` | ✅ success |
| `cargo test --workspace --no-fail-fast` (advisory: some tests need cores the runner lacks) | ✅ success |

