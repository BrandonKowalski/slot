# slot.

A bespoke, GBA-only frontend for the Anbernic RG SP.

Pick a cartridge from the carousel, and it is inserted into the slot with a nostalgic scrape and clunk.

A save state is captured when you power off the device. When you reboot, the game left in the slot will resume immediately.

The only setting exposed is a clock which you can set the first time you run slot. Everything else is managed on the SD Card.

## Controls

On the carousel, `L` and `R` browse and `A` plays.

A short tap of `A` will resume the last save state. Holding `A` will start the game fresh.

`MENU` opens the about screen.

In game:

| Input                      | Action                                                                                           |
|----------------------------|--------------------------------------------------------------------------------------------------|
| Hold `MENU`                | Save state, eject cart, back to the carousel                                                     |
| Double tap `MENU`          | Save state switcher, select which one to load or delete, undo last save / load within 30 seconds |
| `SELECT` + `R1`            | Save state                                                                                       |
| `SELECT` + `L1`            | Load the newest state                                                                            |
| `SELECT` + `Up / Down`     | Brightness                                                                                       |
| `SELECT` + `Left / Right`  | Blue light                                                                                       |
| `L2`                       | Rewind while held                                                                                |
| `R2`                       | Fast forward while held, double tap to toggle on                                                 |
| `VOL+` and `VOL-`          | Change the volume                                                                                |
| `VOL+` and `VOL-` together | Mute, remembering the level                                                                      |

Closing the lid sleeps, and sleeping long enough powers off. Both cases create a save state. 

On the next boot, slot goes straight back into the game.

## SD Card Layout

```
BIOS/         gba_bios.bin, optional. Absent means mGBA's own high level BIOS.
Games/        .gba roms.
Labels/       <rom stem>.png, drawn on the cartridge face. Absent means a text only label.
Saves/        .sav and .srm battery saves.
States/       save state rings, ten deep per cart.
System/       the binary, the core, and theme.txt.
Wallpapers/   .png, one picked at random each boot and drawn behind the shelf.
```

`System/theme.txt` is entirely optional and controls the appearance of the slot:

```
housing #24242a
recess  #1a1a1e
opening #050508
edge    #4d4d57
```

## Building

Needs [Task](https://taskfile.dev). `task` on its own lists everything.

```
task run              # release build against ./sdcard
task sdcard           # make an empty card layout
task test             # the whole workspace
task check            # fmt, lint and test
task dist             # tree to copy onto the card
task dist:device      # the same for aarch64, built in a container
task deploy:device    # build and push onto a device over adb, then restart it
task log:device       # read slot's log back off it
```

`SLOT_NO_CORE=1` starts no emulator, which leaves the slot on screen so the insert can be
watched at full length. `SLOT_TRACE=1` prints frame and audio diagnostics.

`SLOT_TRACE_INPUT=1` writes `input-trace.log` to the root of the card: every event node the
device has and whether it was opened, then a line per button edge with the code it arrived as
and the button it mapped to, or `unmapped`. There is no console on the device, so this is how
a pad that reports codes the table does not know is found. Add it to `launch.sh` on the card,
press every button in a known order, and read the card back.

## Credits

Emulation is [mGBA](https://mgba.io) by endrift, through [libretro](https://www.libretro.com). 

The device boots [AGS-102](https://github.com/BrandonKowalski) a purpose made fork of [BaseOS](https://github.com/pvaibhav/BaseOS) by @pvaibhav.

Type is [Open Sans](https://github.com/googlefonts/opensans), under the SIL Open Font License, and [Nerd Fonts](https://www.nerdfonts.com) symbols by Ryan L McIntyre, under MIT.

The panel mask is derived from the LCD3x shader by Gigaherz, from the libretro shader
collection and released to the public domain. At exactly 3x it reduces to a 3 by 3 table,
which is what ships here rather than the shader.

The sounds are a recording of me shoving a cartridge into my childhood GBA.

## AI Disclosure

This was put together by Claude Opus. I wanted a bespoke frontend for my RG SP and thought that that something extremely focused on GBA would be kind of neat.

mGBA is the real star of the show here. This is provided without support and I will be disabling pull requests and issues. 

Use it, don't use it, I don't care. Figured I should share the end result of all the wasted water.