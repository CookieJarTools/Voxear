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
- `glslc`: the `shaderc` package on Linux, or the Vulkan SDK on Windows

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
`--headless --frames 120` to simulate without a window and print where every Obj ended up.

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
