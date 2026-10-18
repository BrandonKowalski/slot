# Changelog

## [1.8.0](https://github.com/BrandonKowalski/slot/releases/tag/v1.8.0)

- When the SD card turns read-only, SELECT + START repairs it: slot steps aside, checks FAT32 or exFAT, and restarts. (#72, #111)
- Power off, including the shutdown after the lid has been closed for 3 minutes, no longer hangs on a black screen: the Wi-Fi driver is unloaded first, as Restart already does.
- If slot stops responding with the lid closed, a separate watcher powers off a minute after the Lid Off time.
- Run-Ahead is removed. It scrambled saves in games with a 128 KB flash chip, such as Super Mario Advance 4. (#104)
- Turbo A and Turbo B have their own rows on the In-Game controls page and can be moved off X and Y. (#127)
- Loading a state while a game is saving to a 128 KB flash chip no longer writes the save into the wrong half of the chip.
- The cart insertion animation no longer stutters, and a large ROM still loading after the cart seats powers the screen on to a blank panel that brightens until the game appears. (#119)
- On Anbernics with a left stick, the stick works as the D-pad. Left Stick as D-Pad on the Gameplay page turns it off. (#106, #107)

## [1.7.0](https://github.com/BrandonKowalski/slot/releases/tag/v1.7.0)

- The slot, carts and shelf now have a skeuomorphic effect, with light and dark themes and bundled wallpapers. Huge thanks to Markus for this amazing work! (#74, @MarkusMaribu)
- An in-game menu: tap MENU to pause the game behind it. A double tap still goes straight to States.
- Favorites: Y stars a cart onto its own shelf. (#89, @diluno)
- A Recents shelf of the last 10 games, switched on from Settings -> General.
- slot remembers the shelf and cart at power off and boots back onto them. (#53)
- Per-platform bezels. Markus again! (#77, @MarkusMaribu)
- On 4:3 panels the shelf fills the screen. (#81, @diluno)
- Game Titles on the General page fades the shelf name into the selected game's name. (#84)
- A 1 frame run-ahead setting for GBA games on mGBA. (#56, @454135431515)
- Wallpaper Tint darkens your own wallpaper on the shelf.
- GBA, GB and GBC each get their own Shader and Color Correction rows. (#83, @paulocoe-creator)
- LCD3x is offered for GB and GBC games. (#88, @paulocoe-creator)
- Game Boy stretch now properly only widens the picture.
- The backlight steps follow a curve, so the low-end goes dimmer. (#54, @flo333)
- Holding a direction repeats it in menus, the clock and the shelf letter jump, but never in play.
- RG34XX uses its own chords for brightness and the menu display. (#85, @paulocoe-creator)
- The RG28XX picture is turned the way BaseOS says.
- Restart no longer hangs on a black screen: the WiFi driver is unloaded first.
- Muted play keeps its frame rate. (#51, @flo333)
- Fast-forward no longer stutters on frames that take longer than average. (#70)
- gpSP no longer crashes when a crowded boot left its JIT cache unmapped.
- Core options follow the platform that is playing. (#82)
- Card logs moved into a `Logs` folder, and the session log rotates. Card alarms are logged too.

## [1.6.0](https://github.com/BrandonKowalski/slot/releases/tag/v1.6.0)

- Runs on the RG28XX: its panel is mounted portrait, so slot turns the picture onto it. (#35, @GamerAnim)
- Pokémon Pinball: Ruby & Sapphire rumble. mGBA now sees the game's Game Boy Player check. You still need to turn rumble on in game settings. (#25)
- A red alarm shows when the SD card turns read-only or a save fails to write.
- A new shortcut for mute: SELECT + VOL+ / VOL-. The 34XX has a fake volume wheel so VOL+ + VOL- isn't possible. (#39)
- Game Boy cartridge clocks are kept in the .sav. (#22)
- The GBA cartridge clock keeps running on gpSP while the game is off.
- Large libraries now should boot faster, and the first frame shows sooner.
- Games start faster: resting on a cart reads its ROM ahead, so big ROMs no longer wait on the SD card when you insert them.
- Wallpapers are dealt from a shuffled deck, so none repeats until all have shown.
- PNG art is turned the way its EXIF orientation tag says.
- When slot crashes it logs where, and keeps that run's log in `crash/` to attach to a bug report.
- The BaseOS startup sound no longer leaves slot silent. (#20)
- A user defined `Config/bootlogo.bmp` will take precedence over the shipped one. (#30)
- Cart shell, core and video mode files are now `.txt` instead of `.ini`, so Chrome on Windows opens them. slot renames them on first boot. (#15)
- The grid shader's lines stay even on the RG34XX and at fractional scales, and the dot shader no longer darkens a white screen as much. (#26, #36)

## [1.5.0](https://github.com/BrandonKowalski/slot/releases/tag/v1.5.0)

- Rumble regression fixed.
- The boot logo is built to the size of the one BaseOS installed, so it fits every panel instead of only 720x480.
- Runs on the RG35XXSP: the picture fills the 4:3 screen's width and a GBA SP bezel sits underneath while a game plays.
- Screen shaders chosen per platform: Off, LCD3x, Grid or Dot for GBA, and Off, Grid or Simpletex for Game Boy and Game Boy Color.
- SELECT + B / A in game steps back and forth through the screen shaders. A game's own soft reset that holds SELECT with A or B no longer reaches it; hold A on the carousel to start a game fresh instead.
- New settings: auto save on eject, turbo buttons and rewind can each be switched off.
- Settings are grouped into Screen and Gameplay pages, and every value wraps from the last choice to the first.
- Label art is cached on the card, so the shelf no longer stutters building cart faces. New labels are cached once at boot behind a Caching New Labels screen.
- Named palettes for Game Boy games: turn on GB Palettes in the Screen settings, then SELECT + L2 / R2 in game steps back and forth through mGBA's 48 palettes.
- The speaker no longer buzzes while nothing is playing. slot lets go of the audio device after 3 seconds of silence and takes it back when sound starts. Thanks to flo333 for finding it.

## [1.4.0](https://github.com/BrandonKowalski/slot/releases/tag/v1.4.0)

- The volume bar shows a headphones icon while they are plugged in.
- Speaker and headphones keep their own volume and mute, and switch the moment you plug / unplug.
- Settings moved from `System/` to a new `Config/` folder, so replacing `System/` on update no longer resets them. slot moves them on first boot.
- The quick menu and power menu wrap from the last item to the first, and back.

## [1.3.0](https://github.com/BrandonKowalski/slot/releases/tag/v1.3.0)

- The SP boots to a slot logo instead of BaseOS's. slot installs `System/bootlogo.bmp` on first run and keeps the original as `bootlogo.baseos.bmp`.

## [1.2.2](https://github.com/BrandonKowalski/slot/releases/tag/v1.2.2)

- Fix for hitch in games caused by autosave. The autosave now writes to the card in the background.

## [1.2.1](https://github.com/BrandonKowalski/slot/releases/tag/v1.2.1)

- Shell colors from Cart Studio now apply to games with accented names.
- Clear GBA carts no longer show their circuit board above the label.

## [1.2.0](https://github.com/BrandonKowalski/slot/releases/tag/v1.2.0)

- Hold X for turbo A and Y for turbo B.
- Carts on the shelf are larger, and shrink into the slot as they go in.
- Carts are drawn at their real proportions, so full label scans fit without cropping.
- Jumping a letter with Up or Down shows the letter in the slot.
- Saves now stick for games whose save is smaller than the core first reports, such as Golden Sun on mGBA.
- The seated cart no longer shows through the slot as a game starts.
- SELECT reaches the game as soon as it can no longer be the start of a shortcut.

## [1.1.0](https://github.com/BrandonKowalski/slot/releases/tag/v1.1.0)

- Fast-forward runs several game frames per refresh, so it is properly fast, and rewind no longer records while fast forwarding.
- The About label credits BaseOS.

## [1.0.0](https://github.com/BrandonKowalski/slot/releases/tag/v1.0.0)

- Game Boy and Game Boy Color, each on its own shelf. L1 and R1 switch shelves, and carts are drawn at their own size in their own plastics.
- Two cores, mGBA and gpSP. Press START on a cart to open it and swap the chip.
- Link play between two RG SPs over WiFi: the link cable and the Wireless Adapter for Pokémon trades and Advance Wars, mGBA's link mode for Mario Kart, and Game Boy link for Tetris.
- A quick menu on the shelf for fast forward speed and sound, rumble, color correction, the date and time, and About.
- Game Boy games stretch to fill the screen with L1 and go back with R1.
- The real BIOS boot animation plays when the card has one.
- Per-cart shell colors, set in `cart_shell.ini`.
- Up and Down jump through the shelf a letter at a time.
- Your own folder layout on the card is kept, and slot runs on upstream BaseOS.

## [0.1.1](https://github.com/BrandonKowalski/slot/releases/tag/v0.1.1)

- The game holds still behind the power menu.
- Rewind records at every speed and replays exactly what happened.

## [0.1.0](https://github.com/BrandonKowalski/slot/releases/tag/v0.1.0)

- First release: a GBA-only shelf of cartridges with an insert scrape and clunk.
- The game in the slot resumes on the next boot. Closing the lid sleeps and saves.
- Save states with a switcher, rewind, fast-forward, brightness, blue light and volume.
- A power menu on a held POWER, and an About label.
