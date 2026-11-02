# Imported features

This fork selectively ports work by Prashant Vaibhav from
[pvaibhav/slot](https://github.com/pvaibhav/slot), inspected at
`6fa0467` (2026-10-27). The original GPL-3.0-or-later license and third-party
notices remain in place. These are feature ports onto this fork's newer UI and
Japanese localization, rather than a merge of the entire source fork.

## Automatic 120 Hz

Supported RG SP, RG34XX SP v1, and RG34XX panels switch automatically at startup.
The model, panel driver, device-tree timings, and register access must all match
before a switch is attempted. Other revisions keep their existing mode. Writes
are verified, with a rollback attempted if verification fails, and the selected
mode is restored before the backlight on wake.

Emulation and audio follow the core's native timing independently of the display
refresh rate. For diagnostics, `high_refresh=0` in `Config/slot.state` disables
the automatic switch. Hardware validation is still required for each device;
software tests cannot measure input lag or prove panel operation.

## Battery life and heat

The existing launcher already selects the `schedutil` CPU governor, allowing CPU
frequency to follow load. This is the same change requested from the source fork.
Actual power consumption depends on the device, game, core, and enabled features.

## Home Wi-Fi

Copy `Config/wifi.toml.example` to `Config/wifi.toml`, fill in the network details,
and enable **Home Wi-Fi** under **General**. The setting defaults to Off and is
saved across restarts. A Wi-Fi mark appears beside the battery when connected.
See [network configuration and lifecycle](network/README.md) for multiple
networks, supported security, time synchronization, and troubleshooting.

Device bundles include `slot-services` and the required `iw` tools and libraries.
The build and deployment tasks copy only the example configuration, preserving
saved credentials. Building only the frontend binary is insufficient for Wi-Fi.

## Cart labels

Missing GB, GBC, and GBA labels download in the background. ROM checksums are
matched against the bundled LaunchBox index, with a title-search fallback.
Existing labels are preserved. Downloads have bounded timeouts and retries;
after repeated network failures, remaining labels are retried on the next launch.

For offline use, place a compatible `Artwork.pack` in `Labels/`. It is consulted
before network requests. The source fork distributes packs in its releases;
`task labels:pack` can also build one. Artwork is not bundled with this repository.
The pack/index generators live in `crates/slot-labels/examples`.

## Linking over home Wi-Fi

Connect both handhelds to the same home LAN and use the normal Host/Join controls
with the same game. The picker indicates **HOME WI-FI** or **DIRECT LINK**.
LAN hosts are discovered with mDNS (UDP 5353) and connect on TCP 7211. Networks
with client isolation or blocked multicast may prevent discovery. Without a home
connection, the normal direct link remains available; pending home association
is paused for the direct-link session and resumed afterwards.

## Port history and future updates

| Feature | Source commits |
| --- | --- |
| Home Wi-Fi/service | `92f812e`, `a388c4c`, `b977d34`, `3487939` |
| LAN linking | `8700a56` |
| Background labels | `12f500a`, `0387313`, `375b61a`, `bca7dcf` |
| Refresh and native pacing | `e8ff757` |
| CPU governor (already present) | `ad7ce10` |

The service and label crates are separate workspace members. Integration points
are the General menu/state, frontend label worker, link transport, panel power
backend, and device frame pacer. Future ports should preserve this fork's clock,
localization, rendering, and state-migration behavior. Achievements, cheats,
colour calibration, and power-button changes were not included in this port.
