# Building a Release

A release is one zip for every H700 device, named `slot-<tag>-h700.zip`, holding a `slot-<tag>/`
folder whose contents go onto SD2. For example `slot-v1.7.0-gp-custom-h700.zip` unzips to
`slot-v1.7.0-gp-custom/`.

## On GitHub

Pushing a `v*` tag runs [`release.yml`](.github/workflows/release.yml), which builds the tree on an
arm64 runner, checks every binary is aarch64, zips it and attaches the zip to a new release:

```sh
git tag v1.7.0-gp-custom.1
git push origin v1.7.0-gp-custom.1
```

It can also be started by hand from the Actions tab, which builds the zip as a workflow artifact
without publishing a release.

## Locally

You need [Task](https://taskfile.dev), Rust via [rustup](https://rustup.rs) (the version is pinned in
`rust-toolchain.toml`) and Docker running. On macOS:

```sh
brew install go-task cmake
curl --proto '=https' -sSf https://sh.rustup.rs | sh
```

The device build runs in a `linux/arm64` `rust:1-bullseye` container, unless the machine is already
an aarch64 Linux box. Build the tree and zip it the same way the workflow does:

```sh
TAG=v1.7.0-gp-custom
task dist:device OUT="slot-$TAG"
zip -qry "slot-$TAG-h700.zip" "slot-$TAG"
```

The first run compiles mGBA, gpSP and slot and takes a while; later runs reuse `vendor/` and
`target-device/`. The build uses the working tree as it is, uncommitted files included, so commit
first if the zip should match the tag.

To attach a local zip to an existing release:

```sh
gh release upload "$TAG" "slot-$TAG-h700.zip" -R <owner>/<repo>
```

