# Third-party licenses

`slot` itself is MIT (see the repo's top-level `LICENSE`). The release also distributes two
compiled libretro cores it did not write:

| Core            | Source                                   | License  | Text here                |
|-----------------|------------------------------------------|----------|---------------------------|
| `gpsp_libretro`  | https://github.com/libretro/gpsp        | GPL-2.0  | `gpsp-GPL-2.0.txt`        |
| `mgba_libretro`  | https://github.com/libretro/mgba        | MPL-2.0  | `mgba-MPL-2.0.txt`        |

gpSP was originally written by Gilead "Exophase" Kutnick; the libretro core above is the
actively maintained fork slot's fetch script pulls from. mGBA is by Jeffrey "endrift" Pfau.
libretro/mgba is libretro's fork of https://github.com/mgba-emu/mgba.

gpSP is conveyed unmodified, in the executable form the libretro buildbot publishes, fetched by
`taskfile.yml`'s `core:gpsp`. mGBA is built by this repo instead: `cores/mgba/build.sh`, run by
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

- **GPL-2.0 (gpSP): the corresponding source ships in this directory, under section 3(a).**
  Section 3 allows conveying object code three ways: with the corresponding source, with a
  written offer for it, or — noncommercial only — by passing along an offer you received. This
  release takes the first and makes no offer: the source is here, in the same directory and
  the same zip and on the same card as the binary it corresponds to. There is nothing to
  request and nobody to request it from.

  `taskfile.yml`'s `core:gpsp` task fetches the binary, resolves a matching source commit,
  downloads that commit's source archive from GitHub, and records the commit — all as one
  fetch (see below). `dist:device` and `deploy:device` carry the result
  right here, next to this notice, as:

  ```
  licenses/gpsp-<commit>.tar.gz
  licenses/gpsp-<commit>.meta
  ```

  named for the exact commit fetched, so the archive identifies its own source without needing
  a release page to point back to — which matters, because a card built and copied by hand
  never has one. The `.meta` file records, in `key=value` form, the same three facts this
  section explains: the resolved `commit`, the `binary_date` it was anchored to, and which
  `resolved_via` method (`github-api` or `git-ls-remote-fallback`) actually produced it.

  What "corresponding" can mean in practice, stated honestly rather than glossed over: the
  libretro buildbot builds gpSP's `master` continuously and does not publish which commit
  produced a given nightly build — that gap is real and this process does not close it. What
  it does do is anchor the *source's* commit to the *binary's own build timestamp* rather than
  to whenever we happened to fetch: `core:gpsp` reads the timestamp the buildbot itself stamped
  onto the `.so` file inside its nightly zip (closer to actual compile time than the zip's HTTP
  `Last-Modified` header, which we observed move forward by two days against a binary that had
  not changed at all — storage or CDN behavior, not a rebuild), then asks GitHub for
  `libretro/gpsp`'s `master` tip as of that exact moment. That is a materially closer inference
  than "master's HEAD whenever our fetch script happened to run," which could — and, in the
  commit this replaced, did — land on a commit made *after* the binary it was meant to describe,
  which is not "corresponding" source in even a good-faith sense. It is still an inference, not
  a proof: the buildbot could have built from a commit slightly before or after our anchor
  point even with perfect anchoring, since it does not expose which commit it actually built.
  Anchoring by timestamp narrows that gap; it does not eliminate it.

  When GitHub's commit-history API can't be reached — it is unauthenticated and rate-limited to
  60 requests/hour — `core:gpsp` falls back to `git ls-remote`, which can only name `master`'s
  tip *right now*, not as of the binary's timestamp. Which path actually ran is recorded in
  `resolved_via` above rather than left implicit, so a fallback-resolved archive is never
  presented as if it carried the same anchoring as the normal path.

  **The binary and the source are fetched, and refetched, as one set.** `core:gpsp`'s status
  check requires the binary, the source archive, the recorded commit and the metadata all to
  already exist; if any one is missing, all four are cleared and refetched together in the
  same run.
  Earlier, the binary and the source were fetched by independent tasks with independent
  presence checks, so deleting only the `.so` — to pick up a newer nightly, say — would refetch
  a new binary and silently leave it paired with whatever source an earlier run had recorded.
  That coupling is gone: nothing here can pair a binary from one fetch with a source recorded
  by another.
