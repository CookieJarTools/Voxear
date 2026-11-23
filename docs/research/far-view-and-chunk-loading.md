# Far view and chunk loading: why you see less far, and how to load faster

Research for the question: "For the non-infinite world you can't see as far. Is this due to needing to load the
chunk data? Do we generate the LODs at runtime? We should just bake the LODs and load them if that's cheaper. Also
the chunks seem really slow to load in."

Measured on 2026-11-23 on copies of CubeGame (`target/lod-bake/cg`, `.voxear/Editor` removed), master `ffceb2a7`
plus temporary timers that are not committed. The machine was shared: load average 10–24, and the user's editor was
on the GPU. Treat the times as upper bounds; the ratios between them are steady.

## The short answer

- **Yes, every level of detail is made at run time.** Nothing about the far view is baked. Only the bare ground of
  the coarsest rings is kept on disk, as a cache, and in practice that cache is thrown away often.
- **The Realm is not short-sighted because it has a set size.** A world of a set size streams exactly as an endless
  one does, with the same rings. It sees less far because its far view takes **3–5 times as long to make** as the
  Islands world's: 16 biomes, 64 structures, overhangs, caves and rivers. At the project's Medium setting, after two
  minutes the Realm had made its near blocks and part of its first ring, and **no far tile at all**. What lies past
  about 600 m stays empty until then.
- **Baking would help a great deal.** A kept far tile reads in **0.08 ms against 71 ms** to make it, and against
  **~500 ms** with its trees and structures built in. Baked with what is built on them and with your hand edits,
  the whole Medium view (about 2,300 columns) would load in about **10 s on 4 threads** instead of **6 minutes or
  more**. The horizon could reach the whole world.
- **Chunks near the camera are slow because the generator is slow,** not storage, meshing or upload. A near column
  of the Realm takes 465 ms to make and 24 ms to mesh. Uploading took 1.3 s in all over a two-minute run.
- **Hand edits are stored ~100 times larger than they need to be.** Fantasy City's 2.56 million changed blocks are
  a 10.3 MB file (4.0 MB once written again). As runs of equal blocks with zstd, they are **42 KB** and decode in
  2.5 ms. That matters most for sending edits to players who join.
- **A frame-rate bug turned up along the way.** The Realm spends **8.5 ms of every frame** on the main thread
  checking whether its settings changed, which is most of a 13 ms frame.

## Kinds of Block World, and what each one draws far off

| Kind | What it draws | Where the far part comes from | How far you see |
|---|---|---|---|
| **Made whole** (Endless off; flat, or generated up to 4096 blocks a side) | Every block of the world at full detail | All made when the level loads, from the generator or the flat settings, with the `.blocks` edits on top. No coarser levels. | To the world's own edge, at most 4 km with metre blocks. Nothing beyond. |
| **Streamed, endless** (Islands, Fantasy City) | Its own blocks near; drawn-only own blocks out to where the first ring starts; then rings of coarser blocks, each twice as large and reaching twice as far | Rings 1–2 and the drawn-only blocks: the **full generator**, every time. Rings 3 and up: **far tiles**, the generator's heights as runs of blocks, kept on disk as bare ground. Trees, structures and edits are built into every ring **each time it is made**. | The quality's far distance: 4 km at Medium, 8 km at High, 32 km at Ultra, at most 8 levels (16 km at 1 m blocks with the default windows). |
| **Streamed to a set size** (Realm, streamed flat worlds) | The same as endless | The same, cut at a Nothing edge, or with the generator's World Edge sea or cliffs past it | The same, once made |

So the far view is worked out at run time from the generator. Hand edits reach the far view only as they are applied
to each coarse column as it is made (`stream::raise`: an edit shows in a coarse block where it fills an eighth of it).

### What limits the distance

1. **Time to make it.** This is what the user sees. The rings fill from the inside out, and in a heavy world the
   outer ones arrive minutes later.
2. **The quality setting.** CubeGame's `voxear.toml` sets `quality = "medium"`. Medium caps a world's own blocks at
   128 m, its far view at 4 km, and makes **8 columns at once on 4 threads** of the 24 the machine has. The Realm asks
   for 256 m and 16 km and gets 128 m and 4 km.
3. **Eight levels at most** (`MOST_LEVELS`). With metre blocks the coarsest ring ends about 16 km out, whatever the
   setting.
4. **The haze** closes in at the far distance, so the last land fades out. It does not hide anything that is there.

The camera has no far plane, and drawing far rings is cheap on the GPU: all runs here drew in 2–2.5 ms.

### The far-view cache on disk

- **Where:** `<project>/.voxear/FarView/<world GUID>/<hash>/<level>_<x>_<z>.far`, one file per tile, lz4.
- **What is in it:** only the bare ground of rings 3 and up, as runs of blocks per column. It holds no trees, no
  structures, no hand edits, and nothing of rings 1–2 or the drawn-only blocks.
- **Size:** about 6 KB a tile for the Realm and Islands, 170 B for flat ground. On the drive each takes about 8 KB,
  because of the file system's clusters.
- **Why it grows from run to run:** the `<hash>` folder is named by the world's **whole file text**, its generator's
  hash and the tile format. Change any setting, biome, structure or block list, and a fresh folder is started. The
  old folders are never removed. The user's Realm has **10 folders and 127 MB from one afternoon**; the newest has
  only 1,393 tiles. The cache copied from the user's Islands world did not match this build either, so every tile was
  made again.

## Measurements

Each run: `vox render --real-time --wait 0 --profile`, 1280×720, camera 60 m up looking out, 2 minutes, with a
timer on every background job (all times are wall time per job on a worker thread).

### What one job costs (median, ms)

| Job | Realm (16 biomes, structures) | Islands (endless) | Fantasy City (flat + edits) |
|---|---|---|---|
| Near column: blocks from the generator | **465** | 95 | 3.7 |
| Near column: faces and light | 24 | 29 | 17 |
| Drawn-only own blocks, a column | 515 (486 generator, 27 faces) | 139 | 23 |
| Ring 1 column | 589 (**558 trees and structures**) | 198 (33) | 15 |
| Ring 2 column | 671 (648) | 257 (118) | 13 |
| Far tile (ring 3+), made | not reached | 562 (71 ground, **474 woods**) | 10 |
| Far tile, **read from disk** | not reached | **0.08** read, then the woods again: 506 in all | – |
| Far tile, faces | – | 13–18 | 9–10 |

### How fast the view fills (Medium unless said)

| Seconds | Realm, Medium | Realm, High (8 threads) | Realm, Ultra (16 threads) | Islands, Medium | Fantasy City, Medium |
|---|---|---|---|---|---|
| Near blocks all drawn | 40 | 33 | 75 | 10 | 5 |
| Drawn-only own blocks done | 80 | 91 | 106 | 22 | 7 |
| Ring 1 done | not by 125 (341/484) | not by 127 (711/1024) | not by 131 (1030/1764) | 76 | 10 |
| Ring 2 done | – | – | – | 91 | 15 |
| Ring 3 (far tiles) done | – | – | – | not by 124 (324/484) | 18 |
| Everything | ~6 min, estimated from what was left | – | – | ~3 min | **20** |

More threads do not help much as the presets stand: each preset that adds threads also widens its windows, so the
work grows with them. High has twice the threads of Medium and four times the columns.

### Where the main thread's time goes

- **Block Streaming: 8.5 ms a frame on the Realm** at every quality, against 0.3 ms on Fantasy City.
  `BlockMaking::same` compares the world's whole file, its biome and structure tables included, with the last one. It
  costs about 2.9 ms and runs three times a frame (`update_from`'s check for what was built, its check of the layers,
  and its check of whether to start again).
- Taking finished columns in, letting them go, asking for more and showing the rings come to under 0.5 ms together.
- Uploads: 1.3 s in all over 7,200 frames, 27 ms in the worst frame. Medium's 8 MB a frame is about a million quads,
  and a column has 1,300–10,000.

### Hand edits (Fantasy City, 961 chunks, 2,555,647 changed blocks)

| Form | Size | Read |
|---|---|---|
| `.blocks` as saved | 10,279,103 B | 5.6 ms (best of 5) |
| `.blocks` written again by this build | 4,011,286 B | – |
| Runs of equal blocks along the chunk, varints | 215,571 B | – |
| Runs + zstd level 3 | **42,046 B** | **2.5 ms** |
| Runs + zstd level 19 | 36,329 B | – |

Each changed block is kept as a 2-byte place and a 2-byte block, which lz4 hardly shrinks. A builder places blocks in
runs (walls, floors, fills), and 2.56 M blocks are only 63,440 runs.

### Far tiles on disk (the user's Realm, 4,319 tiles)

| Form | Size |
|---|---|
| Unpacked | 152 MB (35 KB a tile) |
| lz4, one file a tile (now) | 25 MB (5.9 KB a tile), **32 MB on the drive** |
| zstd 3, 8×8 tiles to a file | 15 MB (3.6 KB a tile) |
| zstd 19, 8×8 tiles to a file | 13 MB (3.0 KB a tile) |

### What the pop-in looks like

Crops enlarged three times from the 2-minute runs (kept in the shared notes folder):

- **Realm:** at 10 s, only a trapezoid of near ground stands in grey haze; at 30 s the ground ends at the window's
  stepped edge; at 2 minutes the first ring's darker band has come in and nothing lies past it.
- **Islands:** at 40 s, the far hillside of ring 2 stands with its **bare grey rock sides** showing under it, where
  the ring 1 columns in front of it have not come yet: a coarse column is meshed alone, its sides open to air. By
  2 minutes they are filled and the hillside reads whole.

## Ideas, with what they would gain

### Baked far levels

**What to bake.** For each level from 3 up, a tile of runs as now, but **built**: the trees, structures and world
layers in it, and the hand edits lowered into it. Rings 1–2 could be baked as runs too, which loses only their
overhangs and caves; the drawn-only own blocks could be made from the level-1 runs in the same way.

**Load against make.**
- Read and unpack: 0.08 ms a tile. Faces: 13–23 ms a column on a worker.
- The whole Medium view is about 2,300 columns: **~10 s on 4 threads, ~3 s on 16**, against 6 minutes or more for
  the Realm now. It is bound by making faces, not by the drive.
- Keeping the faces on disk too would be 10–15 times the size of the runs (8 bytes a quad and 4 of light, 1,300–8,000
  quads a column) to save those 15 ms a column. That is not worth it on disk. Keep faces in memory for columns just
  left, so turning round costs nothing.

**Size on disk for a large world.** Measured at 3.6 KB a tile with zstd in region files. For the Realm, 51.2 km a
side with metre blocks:

| Levels | Tiles | Size |
|---|---|---|
| 5–7 (2 km and further out) | 3,300 | **~12 MB** |
| 3–4 | 50,000 | ~175 MB |
| 3–7 in all | 53,300 | **~190 MB** |
| 2 | 160,000 | ~560 MB |
| 1 | 640,000 | ~2.2 GB |

A flat world's tiles are about 170 B, so a 20 km flat world's levels 3–7 are about 2 MB.

**How long a whole bake takes.** Each coarser level can be made from the one under it, four tiles into one, in
milliseconds, as long as the finest baked level holds the built ground. So the cost is the finest level: about 0.5 s
a tile at level 3 for the Realm with its structures, so ~40,000 tiles is **~25 minutes on 16 threads**. Levels 5–7
alone, from the generator, take about 2 minutes. Baking can run in the background in the editor, on a command
(`vox bake far`), and when a game is built.

**How far you could then see.** The rings' reach is no longer bound by time. Drawing more is cheap: an earlier
measurement found a 16 km far view drew 118k triangles against 92k at 4 km. Two more levels than the eight now would
show the whole Realm from anywhere in it (64 km).

**Keeping it up to date.**
- A hand edit marks the tile under it at every level as changed. The level-3 tile is built again from the generator
  and the edits under it, and each level over it from the four under it: a few milliseconds each, in the background.
- A changed setting, biome, painted map or layer marks the tiles it reaches, as streaming already does for columns.
- Each tile keeps a stamp of what it was made from, so one made from older edits is known and made again.
- The cache should be named by what makes the ground, not the whole file, and old folders removed.

**Compact saves and the network.**
- Baked tiles come from the generator, the layers and the edits. They need never be sent: a joining player gets the
  compact edits (42 KB for Fantasy City, as runs with zstd) and bakes, or reads tiles shipped with the game.
- The levels from 5 up, about 12 MB for the Realm, are small enough to ship with a game build, so the horizon is
  there on the first frame for everyone.
- Edits stay the source of truth; nothing baked is ever saved over them.

### Faster chunks near the camera

| | Idea | Gain |
|---|---|---|
| **A** | Generator speed: the Realm's near column costs 465 ms against Islands' 95 ms. Find what the biomes and structures spend, as the earlier far-view work did for roads and caves. | The largest lever for near ground. Unknown until profiled. |
| **B** | Trees and structures in rings: built every time (558 ms of 589 for a Realm ring-1 column). Keep them in the baked tiles, and build them once at the coarsest level they show. | Ring 1–2 columns from ~600 ms to ~20 ms. |
| **C** | Drawn-only own blocks from runs, not the full generator: 486 ms of 515 a column now, 384–1,088 columns. | That first minute of work down to a few seconds. |
| **D** | Order: rings get no share of the work at Medium (`at_once / RINGS_ALWAYS` is 0 with 8 at once), so they wait for every drawn-only column. With baked tiles, load the coarsest levels first: the whole horizon in a second, then fill inwards. | No grey void while the near ground is made. |
| **E** | Threads: Medium uses 4 of 24. Let thread count follow the machine, apart from the windows' size. | Up to 4–5× on this machine for the same view. |
| **F** | Region files, 8×8 tiles to a file, zstd: fewer files and half the space on the drive. | 32 MB → 15 MB for the user's Realm cache; reading is already quick. |
| **G** | Keep faces in memory for columns just left. | Turning round or walking back costs nothing. |
| **H** | The GPU: making the draw-only rings there would pay only for ground that is not baked, and physics, edits and saves need blocks on the CPU. | Not needed once tiles are baked. |

## Plan, in stages

Each stage stands alone; the gains are from the measurements above.

| Stage | What | Expected gain | Size |
|---|---|---|---|
| **0. Quick fixes** | `BlockMaking::same` compared once a frame, by pointers and a kept hash. The far-view cache named by what makes the ground, old folders removed. Rings always get a share of the work. | **−8 ms a frame** on the Realm. The cache stops being thrown away by unrelated settings, and stops growing. | Small |
| **1. Built tiles kept** | Far tiles kept on disk with their trees, structures and layers built in, in region files with zstd. | A revisited view's rings 3+ from ~500 ms to ~15 ms a column; cache half the size. | Medium |
| **2. Baked far levels** | A bake of levels 5–7 for the whole world (12 MB for the Realm), made in the background in the editor, by `vox bake far`, and with a game build; levels 3–4 kept as visited. Edits lower into them and mark them changed. Coarsest first when a level loads. | The horizon on the first frame, out to the world's edge with two more levels. | Medium |
| **3. Rings 1–2 and the drawn-only blocks from runs** | Made from baked or kept runs instead of the full generator. | The first minute of the Realm's streaming to a few seconds. | Medium |
| **4. Compact edits** | `.blocks` version 5: runs of equal blocks and zstd. Reads version 4. | Fantasy City's edits 10.3 MB → 42 KB, for saves and for players who join. | Small |
| **5. Generator speed** | Profile the Realm's biomes and structures per node. | Near ground; unknown until measured. | Open |
| **6. Threads by machine** | Thread count from the CPU, apart from the windows' size. | Up to 4× on this machine. | Small |

**Stage 0 is done.** The Realm's frame (3,600 frames at Medium, camera still) went from a median of 12.9 ms to
2.7 ms, and Block Streaming on the main thread from ~9.5 ms to ~0.7 ms a frame. The machine was less loaded in the
second run (load average 5 against 23), but this is main-thread work. Stage 1 is to cover endless worlds as much as
worlds of a set size: every level made once is kept, built, so coming back is a read.

Suggested order: 0, 4, 1, 2, then 3 and 5. Stage 0 and 4 are each a day or less. Stage 2 is what answers the user's
question.

## Not done or not checked

- Timings were taken on a busy machine with the user's editor on the GPU.
- The whole-world bake time is estimated from per-tile costs, not run.
- A Realm made whole (Endless off) was not measured: at 4096 blocks a side it would be made in one go when it loads.
- The Realm's generator was not profiled node by node (stage 5).
- The faces of a column read from a tile were timed as part of a far job, not on their own.
