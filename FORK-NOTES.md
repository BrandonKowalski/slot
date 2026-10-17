# Fork notes

Context for whoever syncs this fork with upstream, human or AI agent, without the conversations
that produced it. Read this before resolving a conflict.

## 1. What this fork is

- **Fork:** `Maxcimkj/slot-gb-custom`, default branch `main`.
- **Upstream:** `BrandonKowalski/slot`, branch `main`. slot is a Rust frontend for Anbernic H700
  handhelds (RG SP, RG34XX, RG35XX family, RG28XX, RG40XX, CubeXX) that plays GB, GBC and GBA
  games through the mGBA and gpSP libretro cores.
- **Purpose:** carry one feature upstream does not have, a **Fit** picture mode for GB/GBC games,
  plus its bezel art, and publish device zips from the fork. The target device is the RG SP
  (720x480 panel, the same size as slot's canvas).
- **Base:** upstream `e5b12ad Add RG35XX-H to the device list, closes #98`, which is after the
  upstream `v1.7.0` tag. Fork releases are tagged `v<upstream version>-gp-custom[.N]`, first one
  `v1.7.0-gp-custom` on `16c72c7`.
- **Sync model:** fork commits sit on top of upstream `main` and are rebased onto it, never merged.
  The plan is a scheduled workflow that mirrors upstream and rebases; a conflict fails the job,
  leaves `main` untouched, and is resolved by an agent in a pull request. That workflow does not
  exist yet.

## 2. Fork-only files (cannot conflict)

| Path | What |
|---|---|
| `FORK-NOTES.md` | This file |
| `RELEASE.md` | How release zips are built and named. Kept out of `README.md` on purpose so syncs never touch it |
| `assets/bezel_gb_fit.png`, `assets/bezel_gbc_fit.png` | Built-in Fit bezel art, compiled in |
| `crates/slot-ui/tests/fit_bezel.rs` | Fit bezel PNGs only draw in the side bars |
| `crates/slot/tests/render_fit_bezel.rs` | Full-frame render of the Fit bezel and of a card bezel in Fit |

**Rule:** do not edit `README.md` or `CHANGELOG.md` in the fork. Fork documentation goes in the
files above. If upstream adds a file with one of these names, rename ours, not theirs.

## 3. Fork changes to upstream files (conflict hotspots)

One commit, `16c72c7 Add new fit mode for gb, gbc and bezels`. All of it serves the Fit mode.

### The feature in one paragraph

GB/GBC games had two modes, Actual (exact 3x, 480x432, bezel shown) and Stretch (720x432, bezel
hidden). The fork adds **Fit** between them: full 480 px height at the Game Boy's own 10:9 shape,
about 533x480, with black bars of ~93 px left and right. L1 steps bigger (Actual → Fit → Stretch,
stops at Stretch), R1 steps smaller (Stretch → Fit → Actual, stops at Actual). The mode is saved
per game in `Config/video_mode.txt` as `<rom stem> = fit`. Fit is the only softened mode, using
sharp bilinear, because 3.33x nearest-neighbour makes rows alternate between 3 and 4 lines. GBA has
no Fit and is never affected, even with `fit` saved for a GBA game.

### File by file

| File | Fork change | Upstream used to | Keep when resolving |
|---|---|---|---|
| `crates/slot/src/video_mode.rs` | `VideoMode::Fit` (between `Actual` and `Stretch`), `"fit"` in `as_str`/`parse`, new `bigger()`/`smaller()`, `source_rect` rewritten as a `match` | `if mode == Actual \|\| GBA { WHOLE_TEXTURE }`, else the stretch rect | The GBA check `(w, h) == (SRC_W, SRC_H)` runs **before** the mode match. Fit rect = `cols = h * OUT_W / OUT_H` texels wide, all `h` rows, centred: `[12/240, 8/160, 216/240, 144/160]` for GB. Imports `OUT_H, OUT_W` from `slot_gfx` |
| `crates/slot/src/app.rs` | New `fits_picture()` (Fit and not GBA) and `softens_picture()` (= `fits_picture()`), next to `source_rect()`. In `Phase::Playing`, L1 → `set_picture(self.video_mode.bigger())`, R1 → `set_picture(self.video_mode.smaller())` | L1 → `set_picture(VideoMode::Stretch)`, R1 → `set_picture(VideoMode::Actual)` | Both arms keep upstream's `if self.slot_owns_the_shoulders()` guard. Fit relies on upstream's `shown_game_bezel()` hiding the bezel only in Stretch (upstream already did this; the fork does not change it). If upstream starts hiding it in other modes, Fit must stay shown |
| `crates/slot/src/frontend.rs` | `slot_ui` import list gains `fit_game_bezel_face` (rustfmt rewrapped the whole `use` block). `BezelSync.image` is `Option<(Console, BezelPick, bool)>`. `compose()` calls `compositor.set_soft_picture(app.softens_picture())` right after `set_game_source_rect`. `sync_game_bezel` matches on `(pick, fits)` | `image: Option<(Console, BezelPick)>`, match on pick only | Bezel table: None/Ags102 → none; Default → `default_game_bezel_face` in Actual, `fit_game_bezel_face` in Fit; `File` → card file in Actual, **none in Fit** (card bezels are drawn for Actual and would cover the picture). The `bool` in the cache key makes a mode change reload the bezel |
| `crates/slot-ui/src/backdrop.rs` | `BEZEL_GB_FIT_PNG`, `BEZEL_GBC_FIT_PNG` via `include_bytes!`, and `fit_game_bezel_face(colour)` after `default_game_bezel_face` | — | Same shape as `default_game_bezel_face`, uses `art::cover_bytes(png, OUT_W, OUT_H)` |
| `crates/slot-ui/src/lib.rs` | `fit_game_bezel_face` added to the `pub use backdrop::{...}` list | — | Keep alphabetical order |
| `crates/slot-gfx/src/shaders.rs` | `GAME_FRAG`: `uniform float u_soft;`, a `soft_texel(uv)` function after `varying vec2 v_uv;`, and `main()` starts with `vec3 c = u_soft > 0.5 ? soft_texel(uv) : texture2D(u_game, uv).rgb;` | `vec3 c = texture2D(u_game, uv).rgb;` | Upstream's screen effects (LCD3x, Grid, Dot, Simpletex) run after that line, unchanged. Use `texture2D` (GLSL ES 1.00 on device; the desktop preamble maps it to `texture`). The float flag and `> 0.5` follow upstream's `u_mode` pattern |
| `crates/slot-gfx/src/pipeline.rs` | `GamePass` fields `u_soft`, `soft`; location looked up with the other uniforms; `set_soft(on)`; `draw_source` sends `Uniform1f(u_soft, 1.0/0.0)` | — | Add `u_soft` to whatever tuple or struct upstream uses for uniform locations |
| `crates/slot-gfx/src/fbo.rs` | `Compositor::set_soft_picture(on)` forwarding to `GamePass::set_soft`, next to `set_fills_frame` | — | — |
| `crates/slot/tests/app.rs` | Updated `l_and_r_change_the_mode_on_a_game_boy_cart_and_not_on_a_gba_one`, `the_picture_mode_is_remembered_per_cart`; new `only_the_fitted_picture_is_softened`, `the_fitted_picture_keeps_its_bezel_and_the_stretched_one_drops_it`, `a_gba_cart_saved_as_fit_still_shows_the_whole_picture` | L1 went straight to Stretch | If upstream changes these tests, keep their new setup and re-apply the three-step L/R sequence |
| `crates/slot-gfx/tests/pipeline.rs` | New `the_fitted_picture_fills_the_height_with_bars_only_at_the_sides`, `softening_gives_every_fitted_game_boy_row_the_same_weight` | — | Appended tests; usually conflict only on `use` lines or helpers |
| `site/index.html` | Two strings describing L1/R1: the "Commit Aspect Ratio Crimes" step and the `L1 / R1` key | "stretches" / "original size" | Take upstream's markup, then re-apply the wording: L1 goes to full height first, then stretched; R1 steps back |

### How to resolve a conflict

1. **Upstream wins on structure.** Take upstream's version of the surrounding code (renames, moved
   functions, new fields, new variants) and re-apply the fork's intent from the table above. Never
   drop an upstream change to make the fork hunk apply.
2. **Likely upstream changes and what to do:**
   - *Upstream adds its own picture mode or changes `VideoMode`:* keep their variant, keep `Fit`,
     and decide its place in `bigger()`/`smaller()` (fit stays between actual-size and stretched).
     Every `match` on `VideoMode` must cover `Fit`. If upstream adds a mode that does what Fit
     does, drop the fork's version, keep `"fit"` parsing as an alias, and say so in the PR.
   - *Upstream changes the L1/R1 bindings or `slot_owns_the_shoulders`:* keep their guard and
     binding, swap only the target to `bigger()`/`smaller()`.
   - *Upstream adds a `BezelPick` variant:* add it to the `(pick, fits)` match. If it is drawn
     for the Actual picture (like `File`), it gets `None` in Fit.
   - *Upstream changes `GAME_FRAG`:* keep their shader and re-insert the `u_soft` uniform,
     `soft_texel`, and the one ternary on the first sample. Check that `u_src`, `u_uv`, `u_rect`
     and `u_fbo` still exist with the same meaning; `soft_texel` depends on all four.
   - *Upstream changes the texture size* (`SRC_W`/`SRC_H`, now 240x160) *or the canvas*
     (`OUT_W`/`OUT_H`, now 720x480): the Fit rect is computed from them, but re-check the pixel
     numbers in the tests (black at x 91 and 628, picture at 95 and 624) and the bar width the
     bezel art assumes (x 0..93 and 627..720).
   - *Upstream rewraps the `slot_ui` import in `frontend.rs`:* take theirs, add
     `fit_game_bezel_face`, run `cargo fmt --all`.
3. **Assumption to keep true:** Fit's side bars are the game texture's own black margins. The real
   cores leave them black (`crates/slot/tests/render_picture_modes.rs` asserts it). The mock core
   (`crates/slot-retro/src/mock.rs`) fills the whole texture, so never test "pixel in the bar is
   black" against the mock.
4. **Commit after resolving:** keep the fork change as one commit with its original message.

## 4. Verify after every sync

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p slot --no-default-features --features device -- -D warnings
SLOT_SILENT=1 cargo test --workspace --no-fail-fast
```

`task check` runs the same set plus the card-script tests. A local device build is
`task dist:device OUT=slot-<tag>` (see `RELEASE.md`).

- Renderer tests are `#![cfg(target_os = "macos")]` (headless GL) and **silently pass elsewhere**.
  A green run on Linux CI proves nothing about rendering; run on macOS.
- **Known failures not caused by the fork:** in `crates/slot-gfx/tests/layout.rs`,
  `a_framed_frame_on_a_4_by_3_panel_is_todays_frame` and
  `a_framed_frame_after_a_native_one_is_todays_frame_again` fail identically on unpatched `e5b12ad`
  on a Mac. They compare against stored frame hashes that probably differ per GPU. Do not "fix"
  them in the fork.

## 5. Releases

See `RELEASE.md`. In short: `release.yml` (upstream's, unchanged) runs on any `v*` tag and
attaches `slot-<tag>-h700.zip`, which holds a `slot-<tag>/` folder for SD2. One zip serves every
H700 device. `.gitignore` ignores `/slot-v*/` and `/slot-v*-h700.zip` (two lines appended at the
end; if `.gitignore` conflicts, keep upstream's file and re-append them).

`pages.yml` is also upstream's. It deploys `site/` on pushes to `main` that touch it, and the repo
has upstream's `CNAME` (`slot-cfw.fyi`). In the fork it either fails or publishes a copy under that
domain setting; disable Pages or the workflow in the fork's settings rather than editing the file.

## 6. Open issues and decisions

- **Tag `v1.7.0-gp-custom` does not build:** it points to `16c72c7`, the first version of the fork
  commit, which compiles in `assets/bezel_gb_fit.png` and `assets/bezel_gbc_fit.png` with
  `include_bytes!` but did not contain them. The fork commit on `main` was amended to add them and
  the two new test files. Release from a new tag on `main`, e.g. `v1.7.0-gp-custom.1`.
- **Not verified on hardware:** the device build has not been run on a real RG SP with a real
  game, in particular scrolling with softening on and the shader on the device's GLSL ES path.
- **4:3 panels** (640x480: RG35XXSP, RG35XX Plus, RG28XX, RG35XX-H): slot scales its 720x480
  canvas to 640x427 there, so Fit fills the canvas height, not the panel's, and thin top and bottom
  bars remain. Fixing it means drawing outside the canvas. Out of scope.
- **Planned, not built:** per-mode bezel folders on the card, `Bezels/<GB|GBC>/actual/` and
  `Bezels/<GB|GBC>/fit/` (PNGs directly in `Bezels/<GB|GBC>/` stay Actual bezels). Undecided:
  whether Fit uses the `fit/` file with the same name as the Actual pick, or gets its own pick (a
  new in-game menu row saved as e.g. `bezel_gbc_fit=` in `Config/slot.state`). Until then card
  bezels are hidden in Fit.
- **Trademark:** the Fit bezel label approximates Nintendo's "GAME BOY COLOR" logo in Arial Bold
  Italic. Keep that in mind before publishing the art more widely.

## 7. Background on slot that the change depends on

- **Picture pipeline:** the core renders into a 240x160 texture (`SRC_W` x `SRC_H`, canvas / 3).
  A GB frame (160x144) sits centred at x 40..200, y 8..152 with black margins. `source_rect`
  returns `[x, y, w, h]` in 0..1 texture units, drawn over the whole 720x480 canvas
  (`OUT_W` x `OUT_H`).
- **Bezels:** picked per console (GB, GBC, GBA), not per game, from the in-game menu (MENU →
  Bezel, Left/Right cycles None → Default → AGS-102 (4:3 only) → files in `Bezels/<GB|GBC>/`).
  Saved in `Config/slot.state` as `bezel_gb=`, `bezel_gbc=`, `bezel_gba=` with `none`,
  `default`, `ags-102` or a file name; a missing file resolves to None (`App::resolved_bezel`).
  A bezel is a 720x480 RGBA image over the canvas; the built-in Actual window is about
  x 126..593, y 32..451. On a 720x480 panel the saved default is `ags-102`, which is not offered
  there, so bezels are off until the user picks Default.
- **Fit bezel art:** both PNGs are 720x480 RGBA, transparent except x 0..93 and 627..720. The
  POWER lamp is cut from the left 93 px of `bezel_gb(c).png` and then moved 8 px left, so it
  ends at x 84 (GB) / 79 (GBC) and clears the picture's edge at x≈93; the label runs bottom to top,
  centred at x≈656, y≈115. Made with ffmpeg. If upstream redraws `bezel_gb.png`/`bezel_gbc.png`,
  the Fit art will not match and should be regenerated the same way.
