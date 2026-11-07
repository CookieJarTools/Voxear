---
order: 6
---

Some of Voxear's systems only some games need, such as networking or water. These are **modules**, and each project chooses which it has. A module the project leaves out costs nothing: none of its code runs, your scripts don't see its API, its components aren't offered in **Add Component**, and nothing of it goes into your builds.

## Choosing modules

Open **Project Settings** from the side bar, the button above Settings. The **Modules** section has a toggle for each:

- **Networking**: hosting, joining and dedicated servers. See [[Playing over the Network]].
- **Guns**: procedural holding, aiming, recoil and reloads.
- **2D Physics**: 2D colliders and rigidbodies. See [[2D Physics]].
- **Vegetation**: foliage, seasons and impostors.
- **Water**: oceans, pools, streams and buoyancy.
- **Steam Audio**: occlusion, sound paths and room reverb.

Every install of Voxear has every module, so turning one on later is just a toggle. Your scripts build again with the new set.

The choice is saved in `voxear.toml`:

```toml
[modules]
networking = true
guns = true
physics_2d = false
vegetation = true
water = false
steam_audio = true
```

A new project starts with no modules. A project made before modules existed has every module but Guns, so nothing it uses goes missing.

## When a module is off

A script that uses a module the project doesn't have fails to build, and the Console says which module to turn on:

```
Net is part of the Networking module, which this project does not have; switch it on in Project Settings
```

An Obj that already has a component from that module keeps it, values and all. The Inspector shows the component dimmed, with a **Turn On** button that switches the module back on.
