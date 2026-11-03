# Steam Audio, vendored

Steam Audio 4.8.1, by Valve, from <https://github.com/ValveSoftware/steam-audio> at the release `v4.8.1`
(`steamaudio_4.8.1.zip`, SHA-256 `4a0aa5ec1176f38f0b0993a37c2259d9e86f27e22d5e24f83ec4c3cb9a1d5449`), under the
Apache 2.0 licence in `LICENSE.md`. Its third-party notices are in `THIRDPARTY.md`, and `TRADEMARK_RIGHTS.md` says
how its name may be used: only to say where this code comes from, never to suggest Valve endorses Voxear.

`include/` holds its C headers, unchanged, which `src/steam_audio.rs` follows. Its prebuilt libraries are not kept in
git: `fetch.sh` downloads the release, checks it, and puts the Linux and Windows 64-bit libraries in `lib/`, where
Voxear loads them while it is developed. Voxear loads the library when the game starts, from beside the game, or
from `lib/` here, or from the path in `VOXEAR_STEAM_AUDIO`; without it, sound plays as it does without Steam Audio.

To move to a newer release, replace `include/` with its headers, update the version and checksum here and in
`fetch.sh`, run `fetch.sh`, and run the crate's tests.
