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

`vox run` builds the project's C# scripts, hosts .NET, creates the project's Objs and opens a window. Add
`--headless --frames 120` to simulate without a window and print where every Obj ended up. Add `--renderdoc` and
press F12 to capture a frame.

```bash
./target/debug/vox render samples/Hello --out frame.png
```

`vox render` draws the project offscreen to a PNG and prints:
- GPU time per render pass
- culling counts: what was outside the view, what was hidden by occlusion, and what was drawn
- how many draws used each LOD

Options:
- `--debug-view lod` colours objects by their LOD, and `--debug-view cascades` by shadow cascade. F2 and F3 do the
  same in the window.
- `--renderdoc` also saves a RenderDoc capture to the project's `.voxear/Captures`.

`samples/Stress` has 1,600 spheres and a wall, for trying culling and LODs.

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
