# slot on ROCKNIX: Powkiddy RGB20SX

Porting notes for running slot on the RGB20SX (Rockchip RK3566) under ROCKNIX,
instead of on an H700 under BaseOS.

## Step 0: probe the device

`probe.sh` surveys the device and changes nothing. Copy it over and run it as root:

```sh
scp tools/rocknix/probe.sh root@<device-ip>:/storage/
ssh root@<device-ip> sh /storage/probe.sh
ssh -t root@<device-ip> sh /storage/probe.sh buttons
```

The `buttons` run asks you to press each button in turn. It stops InputPlumber for
about a minute, so EmulationStation won't react to buttons until it finishes. If the
script is killed partway through, reboot or run `systemctl start inputplumber`. Both
runs save their output to `/storage/slot-probe.txt`.

## Step 1: slot boots to the shelf and plays a game

**Done when:** started from SSH, slot shows the shelf on the RGB20SX panel, every face
button, shoulder button, the D-pad and Menu work, a GBA, GB and GBC game each run at full
speed with sound, saves land on the card, and quitting returns the device to
EmulationStation.

What is known before probing, from the ROCKNIX source (`ROCKNIX/distribution`) and the
mainline device tree (`rk3566-powkiddy-rgb20sx.dts`, `rk3566-powkiddy-rk2023.dtsi`):

| Area | BaseOS on H700 (today) | ROCKNIX on RGB20SX |
|---|---|---|
| Display | Mali closed driver with an fbdev EGL window (`slot-gfx/src/fbdev.rs`) | DRM/KMS. GPU is the Mali closed driver (`libmali`, the default) or Panfrost, switched by `gpudriver`. Neither has an fbdev EGL window. sway owns the screen. |
| Frontend | `System/launch_frontend.sh`, started by BaseOS | `essway.service` runs EmulationStation under `sway.service` |
| Buttons | Anbernic BSP codes: `0x136` Select, `0x137` Start, `0x138` Menu, `0x13a`/`0x13b` L2/R2, D-pad on `ABS_HAT0X/Y` | Standard Linux gamepad codes: `BTN_EAST` A, `BTN_SOUTH` B, `BTN_NORTH` X, `BTN_WEST` Y, `BTN_TL`/`TR` L1/R1, `BTN_TL2`/`TR2` (`0x138`/`0x139`) L2/R2, `BTN_SELECT` `0x13a`, `BTN_START` `0x13b`, FN = `BTN_MODE` `0x13c` on a separate `adc-keys` device, D-pad as `BTN_DPAD_*` keys `0x220`-`0x223` |
| Input ownership | slot reads `/dev/input` directly | InputPlumber merges `gpio-keys-control`, `adc-joystick` and `adc-keys` into a virtual DualSense and hides the originals |
| Sticks | centred on 0, deadzone 2048 | `adc-joystick`, raw 15 to 1023 |
| Audio | ALSA | ALSA with PipeWire in front (ES uses the PulseAudio API) |
| Backlight, battery | `/sys/class/backlight`, `axp2202-battery` | `pwm-backlight`, `rk817` battery. slot already finds both generically. |
| Rumble | `EV_FF` | PWM motor at `pwmchip1`, no `EV_FF` |
| Wi-Fi | RTL8821CS, slot drives `wpa_supplicant` | RTL8723DS (2.4 GHz only), owned by ROCKNIX's network stack |
| Panel | 640x480 or 720x480 | 720x720 |

### Work

1. **Platform detection.** A small module answering "BaseOS or ROCKNIX": `NAME=ROCKNIX`
   in `/etc/os-release`, with a `SLOT_PLATFORM` override. Everything below branches on
   it. Choose the display by platform, not by whether `/dev/fb0` exists: DRM's fbdev
   emulation can create `/dev/fb0` on ROCKNIX too.
2. **KMS display backend** (`slot-gfx/src/kms.rs`, behind the existing `Surface` trait).
   Open the DRM card, pick the connected connector and its preferred mode, create a GBM
   surface, and get an EGL display on GBM with a GLES2 context. Swap = `eglSwapBuffers`,
   lock the front buffer, `drmModePageFlip`, and block until the flip event, then free
   the previous buffer. That keeps the blocking-swap pacing `device_app.rs` is tuned for.
   Load `libdrm.so.2`, `libgbm.so.1` and `libEGL` with `libloading`, as `fbdev.rs` does,
   so the device build gains no crates. This needs sway stopped, because KMS allows only
   one owner of the screen.
   *Why not a Wayland window under sway:* it would mean bringing winit into the device
   build, folding winit's event loop into `device_app.rs`, and an extra frame of
   compositor latency. Keep it as the fallback if KMS turns out to be blocked.
3. **Button table by platform** (`slot/src/input/evdev.rs`). Keep the current Anbernic
   table for BaseOS and add a standard-gamepad table for ROCKNIX, including
   `BTN_DPAD_*` and `BTN_MODE`. The two tables conflict (`0x138` is Menu on one and L2
   on the other), so one shared table can't serve both. `WANTED`, which decides which
   `/dev/input` nodes to open, must follow the table, or the `adc-keys` node holding FN
   is never opened.
4. **Launch script** (`card/rocknix/slot.sh`). Stop `essway`, `sway` and
   `inputplumber`, run slot with `SLOT_ROOT` pointing at its card tree, and start all
   three again when slot exits, crashes or is killed. Step 1 is run from SSH only, so a
   bad build can't leave the device on a black screen at boot.
5. **Switch off BaseOS-only features on ROCKNIX.**
   - OTA updates (they download the upstream H700 release)
   - BaseOS updates
   - Boot-logo install (Allwinner `boot-resource` partition)
   - `baseos-splash`
   - slot's Wi-Fi, link play, web server and RomM, which all assume slot owns
     `wpa_supplicant`
   - The `8821cs` unload before power-off
   - The AXP-specific `led.sh` and `lid.sh`
6. **Packaging.** `task dist:rocknix` builds the same aarch64 binary and cores as
   `dist:device`. ROCKNIX's glibc is newer than the bullseye build box's, so the
   binaries run as they are. The task lays out the card tree with the ROCKNIX launch
   script.
7. **Tests.** Unit tests for platform detection, both button tables, the `WANTED`
   choice, and connector and mode picking (from fixture sysfs trees, like the existing
   `slot-power` probe tests). `task check` stays green, including `lint:device`.

### Questions the probe answers

- Does stopping InputPlumber put the raw nodes back in `/dev/input`, and do the codes
  match the table above? ROCKNIX patches the shoulder buttons on Powkiddy boards.
- Which GPU driver is active, and do `libgbm`/`libEGL` resolve to Mesa or `libmali`?
- The panel mode and refresh rate, against the GBA's 59.73 Hz.
- Does ALSA `default` go through PipeWire, and is `hw:0,0` free?
- Does `systemd-logind` handle `KEY_POWER` itself? If so it would power off or suspend
  underneath slot's own power menu.
- Where the roms partition is mounted, its filesystem, and whether it allows
  executables.

### Later steps (not step 1)

- Autostart slot at boot in place of EmulationStation.
- A layout that uses the square 720x720 panel instead of letterboxing 720x480 into it.
- Left stick as D-pad, normalised from the axis range.
- Rumble on the PWM motor.
- Status LEDs.
- Brightness and volume shared with ROCKNIX's settings.
- Sleep.
- Wi-Fi through ROCKNIX's network stack.
- Link play on a 2.4 GHz channel (`AGS_LINK_FREQ`, after checking that the RTL8723DS can
  run an access point).
- The web server and RomM.
- An update channel for ROCKNIX builds.
