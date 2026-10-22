# When Things Go Wrong

## A game is missing from the shelf

- Is it unzipped? slot doesn't read `.zip` or `.7z`.
- Is it right in `Games/GBA`, `Games/GB` or `Games/GBC`, and not in a folder inside one?
- Did slot boot since you added it? Restart from Settings → About.
- Does it end in `.gba`, `.gb` or `.gbc`? GBA games only show up in `Games/GBA`.

## My save is gone

You probably renamed the game. Saves, states and labels follow the file name, so rename the matching files in `Saves/` and `States/` too.

If you switched a GBA game to the other emulator, your save states are still there. They show up again when you switch back. See [Save States](save-states.md).

## The red alarm

A red alarm means slot couldn't write to your SD card. Your progress isn't being saved while it's up.

| It says | Do this |
| --- | --- |
| Can't write to SD card | Something went wrong with the card. |
| Press SELECT + START to repair | slot can try to fix it. It checks the card and restarts. |
| Charge the battery to repair | The repair needs 20% battery, or the charger plugged in. |

If it keeps coming back, the card might be on its way out. Copy your `Saves/` and `States/` off it while you can.

## The clock screen keeps showing up

slot asks for the time on first boot and whenever the clock reads as long ago. If it shows up every boot, the device is losing time while it's off.

It also shows up if `Config/slot.state` can't be read, since slot falls back to the defaults.

## A label didn't change

slot spots a new label with the same name and redraws it. If it still looks wrong, check the name matches the game exactly and restart.

## Reporting a bug

Please [open an issue on GitHub](https://github.com/BrandonKowalski/slot/issues). It helps a lot if you attach:

- `Logs/slot.log`, the run you're on
- `Logs/slot.log.1`, the run before it. If slot crashed and you rebooted, this is the one
- anything in `Logs/crash/`
- `Logs/slot-card.log`, if it's about the SD card or saves

Tell me which device you have, which version of slot (it's on the About page) and what you did right before.
