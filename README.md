# discord rich presence for DCS

![preview screenshot](assets/preview.png)

## usage

1. put [`dcs-rich-presence-hook.lua`](./dcs-rich-presence-hook.lua) into your DCS user data `Scripts/Hooks/` directory (on windows defaults to `%USERPROFILE%/Saved Games/DCS/Scripts/Hooks`)
2. grab the [latest release](https://github.com/backwardspy/dcs-rich-presence/releases/latest) and run it.

## how it works

the export script sends telemetry every 15 seconds to the server binary over UDP. the server connects to your discord RPC socket and updates the activity whenever new telemetry arrives from the export script.

## todo

contributions very welcome!

- build/package for linux
- ~~add per-module [assets](https://discord.com/developers/docs/rich-presence/overview#assets) to use for the "small image", to display what you're currently flying~~ ✅
