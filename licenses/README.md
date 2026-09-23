# Licenses

`slot` is **GPL-3.0-or-later**. Copyright (C) 2026 Brandon T. Kowalski. The repo's top-level
`LICENSE` is the GNU General Public License version 3, verbatim from the FSF.

**It was MIT until 2026-09-17, and that does not un-happen.** Every commit up to and including
`1a655b8` was published under the MIT license and stays available under it to anyone holding a
copy; the change binds what is released from here on. The reason for it is that a permissive
license had no answer to a closed binary repack of this work, and the GPL does: a fork is welcome,
a fork that ships without its source is not.

The release also distributes compiled libretro cores `slot` did not write:

| Core                | Source                                             | License              | Text here                  |
|---------------------|----------------------------------------------------|----------------------|----------------------------|
| `gpsp_libretro`     | https://github.com/libretro/gpsp                   | GPL-2.0-or-later     | `gpsp-GPL-2.0.txt`         |
| `mgba_libretro`     | https://github.com/libretro/mgba                   | MPL-2.0              | `mgba-MPL-2.0.txt`         |

**Every one of those is why GPL-3.0 was available to take, and it was checked rather than
assumed.** gpSP carries the "either version 2 of the License, or (at your option) any later
version" grant in 36 of its source files and version-2-only wording in none of them, so its
or-later grant is what permits the upgrade. mGBA is MPL-2.0, whose section 3.3 secondary-license
clause exists for exactly this combination. A core under GPL-2.0-**only** could not be shipped
beside a GPL-3.0 `slot`, which is not hypothetical: Gambatte is GPL-2.0-only, with the FSF
template's "or any later version" clause deliberately struck out, and that is now the reason it
cannot be used here rather than anything about how it performs.

gpSP was originally written by Gilead "Exophase" Kutnick; the libretro core above is the
actively maintained fork slot's fetch script pulls from. mGBA is by Jeffrey "endrift" Pfau.
libretro/mgba is libretro's fork of https://github.com/mgba-emu/mgba.

Both cores are built by this repo, and both are patched. `cores/gpsp/build.sh`, run by
`taskfile.yml`'s `core:gpsp`, builds libretro/gpsp at a pinned commit from the source archive
that ships here, with gpSP's own arm64 recipe and the patches in `cores/gpsp/` applied. mGBA
takes the same treatment: `cores/mgba/build.sh`, run by
`core:device` and `core:mgba:host`, builds libretro/mgba at a pinned commit with the patches in
`cores/mgba/` applied. `slot` never links against either. `taskfile.yml`'s `dist:device` task
copies this directory into the shipped tree alongside the cores it licenses, so a card built
from this repo carries the same notice the release zip does.

- **MPL-2.0 (mGBA): this build is modified, and the modifications ship in this directory.**
  The core is libretro/mgba at the commit recorded in `mgba-<commit>.meta`, with every patch
  from `cores/mgba/` applied. Each patch ships here too, its file name prefixed `mgba-`, and is
  itself under MPL-2.0; the rest of the Source Code Form is public at
  https://github.com/libretro/mgba at that commit. That is what MPL-2.0 sections 3.1 and 3.2
  require recipients be told, and this paragraph is that notice.

  The one patch today is upstream mGBA's own fix for the Classic NES Series audio,
  https://github.com/mgba-emu/mgba/commit/685023e05d90d87050fb357f46f7bd2d907083f5, which
  libretro/mgba had not picked up when this build was set up. Once it has, the patch can go.

- **GPL-2.0-or-later (gpSP): the corresponding source ships in this directory, under section 3(a).**
  Section 3 allows conveying object code three ways: with the corresponding source, with a
  written offer for it, or — noncommercial only — by passing along an offer you received. This
  release takes the first and makes no offer: the source is here, in the same directory and
  the same zip and on the same card as the binary it corresponds to. There is nothing to
  request and nobody to request it from.

  `taskfile.yml`'s `core:gpsp` task downloads the source archive of the commit pinned as
  `GPSP_COMMIT` from GitHub, then compiles the binary from that archive with
  `cores/gpsp/build.sh` — all as one set (see below). `dist:device` and `deploy:device` carry
  the result right here, next to this notice, as:

  ```
  licenses/gpsp-<commit>.tar.gz
  licenses/gpsp-<commit>.meta
  licenses/gpsp-<patch>.patch
  ```

  named for the exact commit built, so the archive identifies its own source without needing a
  release page to point back to — which matters, because a card built and copied by hand never
  has one. The `.meta` file is the build's own record, in `key=value` form: the `commit`, the
  `source` archive's URL, the `recipe` it was built with (`make platform=arm64`, gpSP's own
  Makefile target), the `device_cflags` added to that recipe's flags, and a `patch=` line per
  patch with its sha256.

  **This build is modified, and these are the modifications** — that is what GPL-2.0 section
  2(a) asks be carried in the changed files, and this paragraph is the notice. Every patch
  `cores/gpsp/` holds ships here beside the archive, its file name prefixed `gpsp-`. Today
  there is one, slot's own: gpSP never reset its Advance Wars serial state when a netplay
  session began, so a session started while the game already sat on its link screen drained a
  master-side buffer as a slave, underflowed a length and overran a fixed array, which killed
  the frontend. It resets that state when a session starts and ends, and bounds the drain.

  "Corresponding" is exact here, not inferred: the binary is compiled from this archive plus
  those patches, and nothing else. The archive is GitHub's snapshot of `libretro/gpsp` at that
  commit, unmodified, its Makefile included, and the build adds only the patches and the
  compiler flags the `.meta` names. Earlier, slot shipped the libretro buildbot's nightly
  binary, which does not say which commit built it, and could only infer the source from the
  binary's timestamp. Building from the archive closed that gap.

  **The binary and the source are made, and remade, as one set.** `core:gpsp`'s status check
  requires the archive, the recorded commit, the binary and both metadata files to agree with
  the pin and with the build script's stamp. If any one does not, all of them are cleared, and
  the archive is refetched and the binary rebuilt from it in the same run, so nothing here can
  pair a binary from one build with a source recorded by another.

## Artwork

`slot` draws its own cartridges, its own slot and its own wordmark, and the two fonts it sets
type and glyphs in each ship with their licence beside them in `crates/slot-ui/assets/`.

Nothing in this repository is drawn by anyone else. Three Noun Project icons used under CC BY
once were, and the attribution that stood here is in the history with the files it credited.
