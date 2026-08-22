# Third-party licenses

`slot` itself is MIT (see the repo's top-level `LICENSE`). The release also distributes two
compiled libretro cores it did not write, unmodified, fetched by `taskfile.yml`'s `core:gpsp`
and `core:device`/`core` tasks from the official libretro buildbot:

| Core            | Upstream                                | License  | Text here                |
|-----------------|------------------------------------------|----------|---------------------------|
| `gpsp_libretro`  | https://github.com/libretro/gpsp        | GPL-2.0  | `gpsp-GPL-2.0.txt`        |
| `mgba_libretro`  | https://github.com/mgba-emu/mgba        | MPL-2.0  | `mgba-MPL-2.0.txt`        |

gpSP was originally written by Gilead "Exophase" Kutnick; the libretro core above is the
actively maintained fork slot's fetch script pulls from. mGBA is by Jeffrey "endrift" Pfau.

Both cores are conveyed here only in the executable form the libretro buildbot publishes —
`slot` never links against or modifies either. `taskfile.yml`'s `dist:device` task copies this
directory into the shipped tree alongside the cores it licenses, so a card built from this
repo carries the same notice the release zip does.

- **MPL-2.0 (mGBA):** the Source Code Form of the Covered Software is available under the
  same license at the repository above. That is what MPL-2.0 section 3.1 requires and what
  recipients be told, and this paragraph is that notice.

- **GPL-2.0 (gpSP): the corresponding source ships in this directory, under section 3(a).**
  Section 3 allows conveying object code three ways: with the corresponding source, with a
  written offer for it, or — noncommercial only — by passing along an offer you received. This
  release takes the first. `taskfile.yml`'s `core:gpsp:source` task resolves
  `libretro/gpsp`'s `master` commit at fetch time, downloads that commit's source archive from
  GitHub, and records the commit; `core:gpsp` depends on it, so the archive is fetched every
  time the binary is. `dist:device` and `deploy:device` carry the result right here, next to
  this notice, as:

  ```
  licenses/gpsp-<commit>.tar.gz
  ```

  named for the exact commit fetched, so the file identifies its own source without needing a
  release page to point back to — which matters, because a card built and copied by hand never
  has one.

  What "corresponding" can mean in practice, stated honestly rather than glossed over: the
  libretro buildbot builds gpSP's `master` continuously and does not publish which commit
  produced a given nightly build. What we fetch and ship is `master`'s HEAD at the moment
  `core:gpsp:source` runs — the closest identifiable source to what the buildbot actually
  compiled, not a proven match to it. If the buildbot's own fetch lagged ours by even one
  commit, the archive here and the true source of a given `.so` can differ, and we have no way
  to close that gap ourselves — the buildbot does not expose which commit it built. We are
  saying that plainly rather than implying a guarantee this process cannot back.

  **A written offer, section 3(b), stands as a backstop.** For three years from the date of
  the GitHub release that shipped a given `gpsp_libretro.so`, on written request to
  **brandon@kowalski.io** naming that release's tag or the binary's sha256 (both recorded in
  the release notes), the source corresponding to that binary will be provided on a medium
  customarily used for software interchange, for a charge no more than the cost of physically
  performing the distribution. This exists for the case the shipped archive turns out not to
  match: a request naming a specific release lets us go back and check, and provide the actual
  commit if the two diverge from what shipped.
