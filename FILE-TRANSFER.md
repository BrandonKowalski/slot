# Browser file transfer

Connect through Settings → Wi-Fi, then return to Settings and open **File Transfer**.
Slot shows an address such as `http://192.168.1.42:8080/` and an eight-digit code.
Open the address on a computer or phone on the same local network and enter the code.

Choose the destination folder, then drag files onto the page or use **Choose files**.
Multiple files upload one at a time with progress. The page asks before replacing
an existing file. Files on the card can also be downloaded.

Supported destinations:

| Folder | File type |
| --- | --- |
| Games/GBA | .gba |
| Games/GB | .gb |
| Games/GBC | .gbc |
| Labels/GBA, Labels/GB, Labels/GBC | .png |
| BIOS | .bin |

Files must be nonempty and no larger than 128 MiB. Labels should have the same
filename stem as their game. ZIP files need to be extracted on your computer first.
This does not expose System files, Wi-Fi credentials, saves or save states.

Keep the File Transfer screen open and the handheld awake while transferring.
**B** or **MENU** stops the server and returns to settings. Closing the lid,
restarting or powering off also stops it. A new session gets a new code. Restart
Slot after adding games or labels to reload the library.

Uploads are streamed to temporary files on the card. Only completed uploads replace
their destination; a disconnect, cancellation or full card preserves the original.
If a response is lost after completion, refresh the folder to check the result.

The page and server are built into the Slot binary. No extra app, cloud service,
startup script or web server package is needed. This uses HTTP on your local network,
with a session code; it is intended for a trusted home network, not port forwarding.

## Development verification

`cargo test -p slot --no-default-features --test transfer --test transfer_menu`
exercises loopback uploads, downloads, explicit replacement, interrupted writes,
stop during upload, PIN/rate limits, Origin/Host checks, file types, traversal and
symlink rejection, plus settings navigation. Tests use temporary card folders.

`cargo run -p slot --no-default-features --example transfer_server -- <fixture-card>`
serves an explicitly supplied fixture card at `127.0.0.1:8080` for browser testing.
