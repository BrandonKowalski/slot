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

- **GPL-2.0 (gpSP): a written offer under section 3(b).** Section 3 allows conveying object
  code three ways: with the corresponding source, with a written offer for it, or — noncommercial
  only — by passing along an offer you received. Pointing at the upstream repository and
  leaving it there is none of the three: it is not the source itself, and it is not a *written
  offer from the distributor of this binary*, which is what (b) requires. The "conveyed... on
  a physical product... accompanied by a written offer" exception in the third paragraph of
  section 3 does not rescue a bare pointer either, and would not apply to this release even if
  it did — the binary ships on GitHub Releases, not on a physical medium, and the source is
  not offered alongside it there. This paragraph is the actual offer, in its place:

  For three years from the date of the GitHub release that shipped a given
  `gpsp_libretro.so`, on written request to **brandon@kowalski.io** naming that release's tag
  or the binary's sha256 (both recorded in the release notes), the source corresponding to
  that binary will be provided on a medium customarily used for software interchange, for a
  charge no more than the cost of physically performing the distribution.

  What "corresponding" can mean in practice, stated honestly rather than glossed over: the
  libretro buildbot builds gpSP's `master` continuously and does not publish which commit
  produced a given nightly, so there is no commit hash to pin at fetch time. From this
  release onward, each release's notes also record `master`'s HEAD at the buildbot's fetch
  time as the closest identifiable proxy for the commit actually built. Where that proxy and
  the true build commit turn out to differ, we will say so on request and provide the nearest
  source we can identify in good faith — the offer stands regardless of the buildbot's own
  opacity about exactly which commit it built, because the alternative is refusing to convey
  the binary at all, which section 3(b) exists precisely to avoid forcing.
