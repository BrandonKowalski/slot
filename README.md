# slot.

A bespoke, GBA-only frontend for the Anbernic RG SP.

## Controls

### On the carousel

| Input    | Action                     |
|----------|----------------------------|
| `L` `R`  | Browse the carousel        |
| Tap `A`  | Resume the last save state |
| Hold `A` | Start the game fresh       |
| `MENU`   | Open the about screen      |

### In game

| Input                     | Action                                                                                           |
|---------------------------|--------------------------------------------------------------------------------------------------|
| Hold `MENU`               | Save state, eject the cart, back to the carousel                                                 |
| Double tap `MENU`         | Save state switcher: pick one to load or delete, or undo the last save or load within 30 seconds |
| `SELECT` + `R1`           | Save state                                                                                       |
| `SELECT` + `L1`           | Load the most recent save state                                                                  |
| `SELECT` + `Up` `Down`    | Adjust brightness                                                                                |
| `SELECT` + `Left` `Right` | Adjust blue light                                                                                |
| Hold `L2`                 | Rewind                                                                                           |
| Hold `R2`                 | Fast-forward                                                                                     |
| Double tap `R2`           | Lock fast-forward on. Press again to unlock                                                      |
| `VOL+` `VOL-`             | Change the volume                                                                                |
| `VOL+` + `VOL-`           | Mute, remembering the level                                                                      |

Buttons side by side mean either one. A `+` means both together.

Closing the lid writes a save state and turns off the display. Open it again and you're
back in the game. Leave it shut for three minutes and slot powers off, resuming from that
save state on the next boot.

The lid is not a sleep. The panel goes dark but the board keeps running, which is why the
three minutes exist rather than an indefinite standby.

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

Label art is drawn at 196x86, or about 2.28:1. Anything else is scaled to cover that box
and centre cropped, so a square or portrait image loses its top and bottom. Bigger art is
fine and comes down to size; smaller gets stretched up and shows it.

`System/theme.txt` is entirely optional and controls the appearance of the slot:

```
housing #24242a
recess  #1a1a1e
opening #050508
edge    #4d4d57
```

## Installing on your RG SP

1. Download the latest [AGS-102](https://github.com/BrandonKowalski/AGS-102) `.img` release.
2. Use Raspberry PI Imager, RUFUS, et. al. to write the `.img` to an SD Card.
3. Insert this SD Card into Slot 1 of your RG SP. This is the one on the side of the device next to the volume buttons.
4. Download the latest slot release from this repo.
5. Unzip the download
6. Copy all the contents of the zip to a second SD Card
7. Add Games, Saves, BIOS (if you like the boot animation), etc.
8. Insert this SD Card into Slot 2. This is on the side where the power and reset buttons live.

## Updating
I doubt I am gonna work on this more and add to it but in case I do here is how you update.

1. Power off your RG SP.
2. Eject SD Card 2.
3. Connect to your computer.
4. Replace the `System` folder with the `System` folder contained in the update zip.
5. Done.


## Credits

Emulation is [mGBA](https://mgba.io) by endrift, through [libretro](https://www.libretro.com).
The device boots [AGS-102](https://github.com/BrandonKowalski/AGS-102), a purpose-made fork
of [BaseOS](https://github.com/pvaibhav/BaseOS) by @pvaibhav.

Type is [Open Sans](https://github.com/googlefonts/opensans), under the SIL Open Font
License, and [Nerd Fonts](https://www.nerdfonts.com) symbols by Ryan L. McIntyre, under MIT.

The panel mask is derived from LCD3x, a public-domain shader by Gigaherz in the libretro
shader collection. At exactly 3x it reduces to a 3 by 3 table, which is what ships here
rather than the shader.

The sounds are a recording of me shoving a cartridge into my childhood GBA.

## AI Disclosure

The Rust frontend was put together by Claude Opus. I reviewed everything that was
produced. This documentation is 100% free-range, meatbag prose.

The project is extremely low stakes. I wanted a bespoke frontend for my RG SP and thought
that something this focused on GBA would be kind of neat.

This is just a glorified wrapper around mGBA, which is the real star of the show.

Provided without support. I will selectively address filed issues and PRs.

Use it, don't use it, I don't care. Figured I should share the end result of all the
wasted water.