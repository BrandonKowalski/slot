# Wi-Fi on BaseOS

From Slot's cartridge shelf, press MENU and open **Wi-Fi** (between Date & Time
and About). The menu scans for visible networks. Select yours and press A.

For a password-protected network, use the D-pad and A to type. **Y** changes
between lowercase, uppercase and symbols; **X** deletes a character; **START**
connects; **B** cancels. Passwords are case-sensitive. The `SP` key inserts a space.
All printable ASCII characters are available. Open networks connect immediately.

The screen reports connection errors or the assigned IP address. It stays responsive
while scanning or connecting; B returns to settings while the operation finishes.
Only successful connections replace the saved network. Slot reconnects at boot.
**Disconnect** suppresses boot reconnection until you connect again; **Forget** also
removes the saved network. Scanning after disconnect does not reconnect by itself.

Connect your Mac to the same local network, keep the handheld awake, and use an
SFTP client with the displayed IP, port **22**, username **root**, and the password
configured in BaseOS (`root` by default). The Slot card is `/mnt/sdcard`; GBA games
go in `Games/GBA` and labels in `Labels/GBA`. Restart Slot after adding games.

## Supported systems and networks

This implementation targets BaseOS's `wlan0`, `wpa_supplicant`, `wpa_cli`, and
BusyBox DHCP client. Desktop builds display an explanatory message and never
change the computer's network. Other firmware is not supported by this menu.

Visible open and WPA/WPA2 personal networks are supported, including WPA2/WPA3
transition networks that offer PSK. Hidden SSIDs, WPA3-only, WEP, enterprise
authentication and captive portal sign-in are not supported. Non-UTF-8 network
names are omitted. Do not run home Wi-Fi setup alongside a multiplayer Wi-Fi
service; Slot refuses to replace another supplicant.

## Persistence

The active Slot card holds `System/wifi.conf` (one network) and, when disconnected,
`System/wifi.disabled`. Preserve those files when merging future System updates.
There is no additional startup script to install. Network work runs on a background
thread, including automatic reconnection when Slot starts.

The password is converted to a WPA PSK before saving; neither plaintext passwords
nor keys appear in process arguments or logs. The PSK still grants network access:
treat the configuration as private. FAT/exFAT cards do not enforce Unix permissions.

Slot owns a separate supplicant socket under `/run/slot-wifi` and a DHCP client
identified by `/run/slot-wifi-dhcp.pid`. DHCP remains running to renew its lease.
It does not change BaseOS SSH passwords or require changes to the OS image.
