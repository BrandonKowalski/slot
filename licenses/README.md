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

- GPL-2.0 (gpSP): source is the tag/commit the buildbot built, at the repository above; that
  is also where to file for a copy of the exact source behind the binary this release ships.
- MPL-2.0 (mGBA): source is likewise at the repository above; MPL-2.0 requires the Source Code
  Form of the Covered Software be available under the same license, which it already is at
  that repository, and asks that recipients be told this — this file is that notice.
