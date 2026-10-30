# Voxear

A modern, data-oriented game engine: a Rust core, a Vulkan renderer, C# scripting on CoreCLR, and a Unity-style
editor. Every system has an easy API for getting started and an advanced API for full control, and nothing is a black
box.

- [Architecture](docs/ARCHITECTURE.md): what the engine is and how each system works.
- [Roadmap](docs/ROADMAP.md): milestones and open questions.

## Requirements

- Rust (stable, 1.88 or newer)
- .NET 10 SDK: `sudo pacman -S dotnet-sdk` on Arch, `winget install Microsoft.DotNet.SDK.10` on Windows
- A GPU with Vulkan 1.3
- Slang (`slangc`): `sudo pacman -S shader-slang` on Arch, or the Vulkan SDK on Windows
- Optional: RenderDoc (`sudo pacman -S renderdoc`) for frame captures

## Quick start

```bash
cargo build
```

```bash
./target/debug/vox doctor
```

```bash
./target/debug/vox run samples/Hello
```

`vox run` builds the project's C# scripts, hosts .NET, loads the project's `start_level` and opens a window. A
level is a list of scenes (`Levels/*.level`, `Scenes/*.scene`, both plain TOML). In Hello, the Game Manager switches
between a day and a night level every five seconds, and their shared environment scene stays loaded.
`vox assets samples/Hello` lists a project's assets and their GUIDs. Add
`--headless --frames 120` to simulate without a window and print where every Obj ended up.

In the window:
- **Fly around:** hold the right mouse button. WASD moves, Q/E go down and up, Shift goes faster, and the mouse wheel
  changes speed. These are Unity's scene-view controls.
- **Debug views:** F2 shows LODs, F3 shadow cascades, F4 lights per cluster, and F5 normals.
- **Frame capture:** F12 captures a frame in RenderDoc when run with `--renderdoc`.
- **Shader hot reload:** shaders in `crates/voxear_render/shaders` reload on save.

```bash
./target/debug/vox render samples/Hello --out frame.png
```

`vox render` draws the project offscreen to a PNG and prints:
- GPU time per render pass
- culling counts: what was outside the view, what was hidden by occlusion, and what was drawn
- how many draws used each LOD
- the exposure auto exposure settled on

Options:
- `--debug-view lod` colours objects by their LOD, and `--debug-view cascades` by shadow cascade. F2 and F3 do the
  same in the window.
- `--post neutral` or `--post cinematic` swaps the scene's post-processing for another preset, to compare with the
  default Voxear Look.
- `--renderdoc` also saves a RenderDoc capture to the project's `.voxear/Captures`.

Post-processing is on by default: TAA, bloom, auto exposure, AgX tonemapping, a soft vignette and height fog. A
scene changes it in a `[post]` section; `samples/Lights` has an example, and
[Post-processing](docs/ARCHITECTURE.md#post-processing-and-the-voxear-look) lists every setting.

More samples:
- `samples/Stress` has 1,600 spheres and a wall, for trying culling and LODs.
- `samples/Lights` has 256 coloured point lights at night, for clustered lighting.

`--debug-view` takes `lod`, `cascades`, `lights` or `normals`.

Set `VOXEAR_LOG=debug` for more logging. Set `VOXEAR_VALIDATION=1` to force Vulkan validation; debug builds use it
automatically when the `vulkan-validation-layers` package is installed.

## Layout

```
crates/     Rust engine, one crate per system (ecs, scene, rhi, render, scripting, engine, cli)
managed/    C# scripting API (Voxear.Core)
samples/    example projects
docs/       architecture and roadmap
```

## Licence

Voxear is licensed under the [PolyForm Shield License 1.0.0](LICENSE.md): you may use, change and ship games
with it, but not use it to build a product that competes with Voxear itself.

Required Notice: Copyright 2026 Joseph Cook
