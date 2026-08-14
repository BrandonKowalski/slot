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

| Input                     | Action                                                                                           |
|---------------------------|--------------------------------------------------------------------------------------------------|
| Hold `MENU`               | Save state, eject cart, back to the carousel                                                     |
| Double tap `MENU`         | Save state switcher, select which one to load or delete, undo last save / load within 30 seconds |
| `SELECT` + `R1`           | Save state                                                                                       |
| `SELECT` + `L1`           | Load the most recent save state                                                                  |
| `SELECT` + `Up / Down`    | Adjust Brightness                                                                                |
| `SELECT` + `Left / Right` | Adjust Blue light                                                                                |
| `L2`                      | Rewind while held                                                                                |
| `R2`                      | Fast forward while held, double tap to toggle on                                                 |
| `VOL+` / `VOL-`           | Change the volume                                                                                |
| `VOL+` and `VOL-` | Mute, remembering the level                                                                      |

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

## Installing on your RG SP

1. Download the latest [AGS-102](https://github.com/BrandonKowalski/AGS-102) `.img` release.
2. Use Raspberry PI Imager, RUFUS, et. al. to write the `.img` `to an SD Card.
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