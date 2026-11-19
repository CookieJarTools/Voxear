# Crowds: many animated characters in one world

Research for running hundreds to thousands of animated AI characters in Voxear's streamed worlds of blocks and in
mesh levels, written 2026-11-19. Nothing here is built yet. It covers what the engine does now and where it
stops (measured), the techniques that are published and what each costs, and a recommended plan in stages.

**Progress.** Stages 1 and 3 are built (2026-11-19). Box characters are posed by their bones where they are
drawn: 5,000 of them take 2.0–2.5 ms of GPU frame and 4.1 ms of renderer CPU, where past 1,638 used to be drawn
at rest. Tiers put 2,000 far agents' step at 3.8 ms against 5.2 ms all near (along the nav), and 7.8 against
12.8 ms (by Character Controller). The roadmap's Crowds entry under M9 has the details.

## The short version

- **Today's limit is about 1,600 box characters or 125 humanoids posed in a frame.** The skinning pass is cheap.
  What stops it is a fixed cap of 16,384 bone matrices a frame. After that come the three posed copies each
  character keeps in the shared vertex pool, and the CPU work of feeding both. Past the cap, characters are drawn
  unposed. Nothing warns about it.
- **Box characters need no skinning at all.** Every box is bound whole to one bone. One matrix per bone, read
  where the vertex is drawn, poses it exactly. That ends the posed copies and the skinning pass for them, and
  makes 5,000 box characters on screen a realistic aim.
- **Humanoids need tiers.**
  - Near: compute-skinned and motion matched.
  - Middle distance: posed where they are drawn, from shared or rarely updated poses.
  - Far: played back from clips baked into a texture, entirely on the GPU, drawn as pictures furthest out.
- **The CPU side scales already, up to a point.** Nav Agents moving along the nav step 1,000 agents in 1.8 ms.
  What is missing is any notion of importance:
  - every agent is stepped, avoided and animated every frame, however far away;
  - nothing happens to agents when their ground streams out.
- **Co-op can carry several hundred moving agents a player with today's budget.** That needs smaller updates and
  rates by distance. Animation is never sent: each machine works it out from how the agent moves.

**Numbers to aim for:**

| Tier | Where | How many |
|---|---|---|
| Near | Within about 40 m | 200 fully animated (motion matched or Mob Motion, IK, Character Controller) |
| Middle | 40–150 m, on screen | 1,000 more at cheaper animation |
| Far | To the far view | 5,000 more as baked or rigid animation |
| Simulated | The rest of the world | 10,000 kept as compact records, ticked rarely |
| Networked | Per player, at 64 KB/s | 1,000 sent |

## What the engine does now

### Skinning (`voxear_render::skinning`, `shaders/skinning.slang`)

- **Each frame, every Obj with a `Pose` and a skinned `ModelRenderer` is skinned on the GPU.** One compute
  dispatch runs per skinned part, 64 vertices a thread group. It moves each vertex by up to four bones into
  posed vertices in the shared vertex buffer. Every pass then draws the posed vertices in place of the mesh's:
  depth, forward, the sun's cascades, virtual shadows, motion vectors.
- **Each skinned part keeps three runs of posed vertices,** written in turn. Last frame's run stays for motion
  vectors while the frame still in flight reads the one before. They come out of the same 8M-vertex pool as
  every model and block lighting, at 60 bytes a vertex.
- **Bone matrices** for all parts go into one upload buffer of `MAX_SKIN_MATRICES = 16 * 1024` a frame. Each
  skinned part sends its own skin's joints, so a model with two skinned parts on a 66-bone skeleton sends 132.
  When the buffer is full the loop stops. Everything after it is drawn from its mesh's own vertices, at rest.
- **What is skinned:**
  - Everything with a pose is skinned, seen or not. Culling happens later, per instance and per meshlet, against
    bounds widened by how far the pose reaches.
  - Posed instances are left out of the shadow cache and of baked light.
  - They are not in the ray-traced scene.
- **The CPU side** (`Skinning::prepare`) walks every posed Obj on one thread. For each one it builds a
  `HashMap` entry, the matrix rows and a job. Animation writes a new `Pose { bones: Arc<[Mat4]> }` component for
  every Obj every frame, which is an allocation per character.

### Box models

- **What one is.** A `.boxmodel` becomes one skinned mesh with every box's corners bound whole to its bone,
  through the same path a `.glb` takes. The infinite-world template's player has 10 bones and 240 vertices.
- **What poses it.** Mob Motion poses it from how its Obj moves, and clips are baked to the same `Clip` an
  imported model plays.
- **Raised layers and cubes** add vertices but are still bound whole to one bone each, and carry a flat coarser
  LOD of themselves.

### Animation (`voxear_engine::animation`)

- **Mob Motion (`animate_mobs`)** is a pure `MobPoser` per Obj. It clones its component and state each frame and
  runs one Obj after another on the main thread.
- **The Motion Matcher (`match_motion`)** also runs one after another.
  - Its search is a brute-force scan of a 10-number feature per frame of the clips, ten times a second
    (`search_every = 0.1`).
  - Each frame it also samples and blends the matched clips, keeps the feet planted with IK, turns hips against
    chest and writes the pose.
  - The search is the cheap part. The pose work every frame is the cost that grows with the crowd.
- **The plan already names a GPU crowd search** under M6, not started.

### Navigation (`voxear_engine::nav_agent`, `voxear_nav`)

- **Nav Agents step in the fixed step.** Each takes the way it asked for last step and asks again when needed.
  Searches have a budget a step (`searches_a_step = 64`, 60,000 places a search).
- **Each agent then steers.** After that, all agents choose their velocities together by reciprocal velocity
  obstacles, over a grid of cells, on the terrain's threads.
- **Each agent then moves** by one of three ways:
  - its Character Controller, through physics;
  - along the nav, sliding over the places, with no physics;
  - not at all, leaving it to a script or the motion matcher.
- **Every agent with authority is stepped every fixed step, at full rate, wherever it is.** Block worlds already
  have a coarse graph of regions per chunk, which long searches use. Mesh levels have a baked nav mesh with cuts.
- **There is no notion of an agent whose ground has streamed out.**

### Networking (`voxear_engine::net`)

- **The server sends each player a snapshot every tick.**
  - The rate is 60 Hz by default, at most 1,100 bytes a snapshot, within 64 KB/s a player.
  - Objs that changed go in, the longest waiting first, so an Obj left out gains priority.
  - A `NetObject` has a priority and a range, and only Objs in a dimension the player has are sent.
- **A moving Obj costs about 16 bytes an update:**
  - a varint id;
  - a 6-bit change mask;
  - a 74-bit position, millimetres from the level's middle;
  - a 32-bit rotation.
- **A snapshot carries about 65 moving Objs,** so a player gets about 4,000 Obj updates a second. Deltas
  against acknowledged state exist only as resending what was lost, not as smaller numbers.
- **Animation is not sent,** and needs not be. Mob Motion and the Motion Matcher both work from how the Obj
  moves, so each machine animates what it sees move. Nav requests from clients go to the server
  (`Message::NavGo`).

## Measured limits

**How these were measured:**
- RTX 3070, shared with other sessions. Every figure is the least of 60 frames, after 20 frames to settle.
- Test builds: the engine's crates at `opt-level = 1`, as `cargo test` builds them. A release build will be
  faster on the CPU side.
- Rendering: offscreen at 1920×1080 with default settings and a grid of characters all in view. Each character
  got a new pose every frame.
- The measurement tests were deleted afterwards.

### Rendering: posing and drawing N characters

The box character is the infinite-world template's player: 10 bones, 240 vertices. The humanoid is a free
mannequin: 66 bones, about 8,500 vertices in two skinned parts.

| Characters | Skinning pass | GPU frame | Renderer CPU | Pose CPU | Posed |
|---|---|---|---|---|---|
| none | — | 1.2 ms | 0.8 ms | — | — |
| 100 box | 0.025 ms | 1.0 ms | 1.1 ms | 0.04 ms | all |
| 500 box | 0.11 ms | 1.3 ms | 2.4 ms | 0.19 ms | all |
| 1,000 box | 0.21 ms | 1.6 ms | 4.1 ms | 0.38 ms | all |
| 2,000 box | 0.34 ms | 1.9 ms | 6.4 ms | 0.78 ms | **1,638** |
| 4,000 box | 0.34 ms | 2.1 ms | 7.6 ms | 1.5 ms | **1,638** |
| 100 humanoid | 0.25 ms | 1.4 ms | 1.7 ms | 0.18 ms | all |
| 250 humanoid | 0.32 ms | 1.7 ms | 2.0 ms | 0.47 ms | **126** |
| 1,000 humanoid | 0.32 ms | 2.6 ms | 2.6 ms | 2.0 ms | **126** |

"Renderer CPU" is `Renderer::render` less its wait for the GPU. "Pose CPU" is making the `Pose` components, the
least the animation side costs before any animating.

**What the numbers say:**

- **The bone-matrix cap is the first wall.** 16,384 ÷ 10 bones is 1,638 box characters. 16,384 ÷ 132 is 124
  humanoids. Past it, characters are silently drawn at rest.
- **The vertex pool is the second wall, for meshes.**
  - 100 humanoids took 2.56M of the pool's 8.4M vertices, 25,600 each for three runs of 8,500.
  - Block lighting may hold 75% of the pool, so in a world of blocks the guaranteed room is about 2.1M. That is
    about 80 humanoids before lighting starts giving chunks back.
  - A box character takes 720, so the pool would hold over 11,000 of them. They hit the matrix cap first.
- **Small characters are bound by dispatches, not by vertices.**
  - 126 humanoids are 1.07M vertices, skinned in 0.32 ms.
  - 1,638 box characters are 0.39M vertices, skinned in 0.34 ms, which is four times the cost a vertex.
  - Each box character is a dispatch of four thread groups, with a bind and push constants of its own.
- **The renderer's CPU work grows by about 3.5 µs a box character:** the per-Obj walk, the hash map and the
  matrix rows. That makes 4 ms at 1,000, on top of animating them.
- **The GPU cost of drawing is modest.** A thousand box characters add about 0.4 ms to the frame, shadows
  included, so draw cost is not the problem at these counts.

### The CPU: agents and animation

| What | 500 | 1,000 | 2,000 | 5,000 | 10,000 |
|---|---|---|---|---|---|
| Nav Agents along the nav, a step | 1.0 ms | 1.8 ms | 5.0 ms | 8.9 ms | — |
| Nav Agents by Character Controller, a step | 2.3 ms | 4.5 ms | 11.8 ms | — | — |
| Mob Motion posing box characters, one thread | — | 0.34 ms | — | — | 3.7 ms |

**Setup:**
- Agents cross a flat field of blocks both ways, so every agent has neighbours to avoid.
- The step is `step_agents` alone: searches, steering, avoidance and moving.

**What the numbers say:**

- **Along the nav, an agent costs about 1.8 µs a step.**
  - Past 1,000 it grows faster than the count, as denser crowds have more neighbours to avoid.
  - Moving by Character Controller costs 2.5 times as much, as each agent goes through physics.
- **Mob Motion is cheap,** about 0.35 µs a character. It is single-threaded, though, and it is the easy path.
- **Motion matching costs far more a character.** It is a scan of every clip frame ten times a second, then
  clip sampling, blending and two-bone IK on a 66-bone skeleton every frame. It was not measured here: it needs
  a full game with clips. Published figures for one character's search and pose put it in the tens of
  microseconds, about 1 ms per 100 characters on one thread before posing. Hundreds of motion-matched
  characters a frame is not a realistic aim. It is the near tier's tool.

## The options

### Posing on the GPU: three ways, each with its place

**Compute skinning into posed vertices (what the engine does).**
- How it works: each vertex is skinned once a frame and every pass reads the result.
- When it pays: when a character is drawn many times (depth, the main view, shadow cascades, motion vectors).
  It is also the only way to give ray tracing posed geometry, refit into its acceleration structure.
- What it costs: memory a character, three copies of its vertices here.
- Where it fits: the near tier, and only for characters that are seen or cast a shadow seen.
- How others bound it: engines that do this cap the room in megabytes and fall back to the next way beyond it.

**Posing where the vertex is drawn.**
- How it works: the mesh shader reads the instance's bone matrices and weights and poses each vertex as it
  draws, with no posed copy at all.
- Its cost: the skinning is repeated in every pass that draws the character. With four cascades and a depth
  pass that is about six times.
- Its gain: memory a character is only its matrices, 48 bytes a bone.
- Meshlet culling: it needs bounds widened by the pose's reach, which the engine already has.
- Where it fits: the middle tier, and everything of the box kind.

**Rigid parts: box models.**
- **Why they are a special case.** A box model is boxes, each bound whole to one bone. Posing a vertex is one
  matrix multiply by its bone's matrix, with no weights and no blending of matrices, so it is exact, not an
  approximation.
- **What changes.** Read the bone in the mesh shader, as above, but with one bone:
  - the 720-vertex posed copy shrinks to 10 matrices;
  - the skinning dispatch disappears;
  - every box character of one model shares one mesh and its meshlets, drawn as instances.
- **What it costs.** Matrices only: 5,000 characters of 10 bones are 2.4 MB a frame.
- **Raised layers and cubes** are bound whole to their bone too, so this holds for them.

**Shared poses.**
- **How it works.** Characters far enough away, walking or idling, need not each have a pose of their own.
  Evaluate a few dozen poses a frame (a walk at several phases, an idle at several phases) and give each
  character the nearest one, with its own start offset so the crowd is not in step.
- **What it saves.** Matrix uploads drop from one set a character to one set a shared pose. With compute
  skinning, a posed copy can be shared too.
- **Where it fits.** The middle tier's way to go from hundreds to thousands.

### Baked animation

**Bone matrices baked into a texture.**
- **How it works.** Every frame of every clip is stored as bone matrices in a texture, or a storage buffer.
  Each instance carries only its clip, its start time and its speed. The mesh shader reads the matrices for
  its frame, filtering between frames.
- **What it costs.** About 8 MB for forty clips (published, 2008), less with clips packed
  together. Nothing a character but a few numbers, and no CPU animation at all.
- **What it cannot do.**
  - IK and planted feet: feet slide a little on slopes.
  - Look-at.
  - Blending more than two clips cheaply.
- **Where it fits.** Far crowds and the many ambient characters: villagers in a far town, a herd on a hill,
  fish in a school. Published crowds of about 10,000 characters at 30 fps on 2007 hardware used this, and the
  hardware of today is many times faster.

**Vertex positions baked into a texture.**
- **How it works.** Every vertex's position at every frame is stored, so no bones are read when drawing.
- **What it costs.** Memory grows with vertices times frames, so it suits only very small meshes and short
  loops.
- **What it cannot do.** Blend between clips.
- **Is it worth it here?** No. Bone textures do what it does with blending and far less memory, and box models
  are already cheaper than either.

**Pictures far out.**
- **How it works.** A character is drawn as a camera-facing quad from an atlas rendered from many directions.
  The engine has these for plants (`impostors.rs`, hemi-octahedral).
- **What it costs.** Animated, it needs an atlas per clip frame, which multiplies memory.
- **Where it fits.**
  - Box characters past a few hundred metres are a few pixels, and a rigid mesh at its flat LOD is cheap
    enough.
  - Detailed humanoids may want a coarse mesh on baked bones instead.
  - Pictures are the last step, for the far view only.

### Animation level of detail

**Rate.**
- **How it works.** Pose far characters less often: every frame near, every second or fourth further out, then
  every eighth. The GPU blends between the last two poses so motion stays smooth. This is cheap with poses kept
  in a buffer the shader reads.
- **Holding to a budget.** A budget in milliseconds, spread over characters by importance, keeps the total
  steady however many there are. An engine that does this reports root motion stepping at low rates. Root
  motion should stay at full rate and only the pose be updated less often.

**Work.** Further out, drop in this order:
1. IK and planted feet;
2. look-at;
3. fingers and face bones (a reduced skeleton);
4. blending more than two clips;
5. motion matching itself.

Published figures from shipped crowds:
- **One crowd, three tiers.**
  - Near: full animation on 300 bones, up to 40 characters.
  - Middle, about 12–40 m: a cheaper puppet at about 150 µs a character.
  - Far: 11 bones, a fixed fast cycle, under 25 µs.
  - The average cost a character fell to a hundredth of the earlier system's.
- **Another, sorted by distance into five levels.** The nearest ten were 130k triangles all together. The
  furthest hundred were 25k all together.

**How motion matching degrades.** Motion matching is the engine's chosen direction, and it degrades in steps:
1. **Near, about 40 characters:** the full Motion Matcher, searching ten times a second, IK, foot planting,
   actions.
2. **Middle:** the same Motion Set, but the search every 0.3–0.5 s and only on large changes. Two clips
   blended, no IK, posed every second or fourth frame.
   - The search's result is a clip and a phase, and that is what the next tier needs.
   - Searches for many characters together suit the GPU: the database is shared and the scans are
     independent. This is the "GPU crowd search" on the roadmap, worth doing once the middle tier has
     hundreds.
3. **Far:** no search. The Motion Set's states (idle, walk, run) are baked, and the character plays the state
   its speed picks, at the phase its distance walked gives. The feet stay in step with the ground without IK.
4. **When a character comes nearer it moves up a tier,** the matcher starting from the clip and phase it was
   playing. Inertialization makes the switch invisible.

Box characters follow the same tiers with Mob Motion in place of matching. Mob Motion is cheap enough
(0.35 µs) that it can run for thousands, if it runs on threads at a lower rate further out.

### The CPU side

**Importance and rates.**
- **What it is.** Each agent gets an importance from its distance to the nearest player (any player, in co-op),
  whether it is seen, whether it is in a fight, and a priority set by the game.
- **What it sets.** Importance picks a tier, and the tier sets everything else:
  - how often the agent's brain ticks (scripts' Update for it, decisions);
  - how often it steers and avoids;
  - whether it moves by Character Controller or along the nav;
  - how it animates;
  - how often it is sent to players.
- **Published practice.** Shipped crowds keep a small set of fully simulated agents (tens), a few hundred
  "real" ones, and thousands that are cosmetic. One shipped horde ran about 100 drawn for every 6 simulated and
  every 1 with a full brain.

**Movement.**
- **Character Controllers only near.** Moving through physics costs 2.5 times as much and is only needed where
  the agent is pushed, knocked back or seen closely.
- **Avoidance at lower rates.**
  - Mid agents along the nav avoid at 20–30 Hz.
  - Far agents do not avoid each other at all and only keep apart by a cheap grid.
  - Published figures for velocity-obstacle avoidance run 10,000 agents at 30 Hz on a desktop CPU.

**Searches in bulk.**
- **What is already there.** A budget a step and searches on threads.
- **Many agents going to one place,** a raid on a town or a herd to water: one field of directions over the
  regions near the goal, made once and shared, is far cheaper than a search each. The engine's regions and
  their edges are already the coarse layer such a field needs.

**Far agents walk the regions, not the places.**
- **How.** An agent that no one is near moves from region to region along the coarse graph, at the speed it
  would walk. Its position is the region's crossing points, with no steering and no avoidance.
- **What it costs.** A few hundred nanoseconds a step, ticked once a second.
- **When it comes near again,** it is placed on the nearest place in its region and steps as usual.

**Layout and threads.**
- **What is single-threaded today.** Mob Motion, the Motion Matcher and the renderer's skinning set-up loop over
  Objs one at a time, cloning components.
- **What to change.**
  - Keep each tier's agents in flat arrays, updated in parallel over the terrain's or a crowd's threads, with
    results written back once.
  - Keep poses in one buffer, with an index a character, not a component allocated each frame.

### Agents in a streamed world

**What a world of blocks has.** It streams in rings round the players. Agents need the same rings.

**The near and middle rings** hold agents as Objs: prefabs with their components, as now.

**Past the middle ring an agent is a record, not an Obj.**
- **What it holds:**
  - its prefab;
  - where it is (a region and a place);
  - its goal and state;
  - its seed;
  - the few fields the game marks as kept.
- **What it costs.** Tens of bytes each, kept in a store by chunk.
- **When records tick.** Once a second, or only when something asks: a town's day, a dungeon's growth.
- **Where it is drawn.** A record in view of the far view can still be drawn by the far tier's baked animation,
  from the store, with no Obj.

**When ground streams out**, its agents become records. When it streams in, records near enough become Objs
again, a few a frame, as replication already spreads spawns over ticks (`SPAWNS_A_TICK`).

**Saves** keep the records, compact and sent to joining players as they are.

**Ambient life:** villagers, birds, fish, a herd grazing. These can be pure records with a seed and never
become networked Objs, until something interacts with them.

**Mesh levels** are bounded, so they need only the tiers, not the records. Dungeons entered by Portals are their
own dimensions, which the engine already streams and sends apart.

### Rendering at scale

**What the engine already has.** Its GPU culling, meshlets and mesh shaders already do most of what crowd
rendering needs.

**What changes:**

- **One draw a model, not one a character.** Instances of one model share its mesh and meshlets, drawing posed
  from a matrix or pose index. Today each skinned instance draws its own posed copy, so meshlets are shared
  only in their indices.
- **Skin only what is seen.**
  - Use last frame's culling results, for the camera and for the shadow cascades, to pick which near characters
    get compute-skinned this frame.
  - A character that is not seen keeps its pose without being skinned.
- **Posed room as a budget, not a share of everything.**
  - Give posed vertices a pool of their own, sized in Project Settings (for example 1.5M vertices,
    90 MB, three runs of each character counted), so they no longer fight block lighting for the shared pool.
  - A character that does not fit falls back to being posed where it is drawn, rather than drawn at rest.
- **One skinning dispatch for all.** A table of jobs, one thread group a 64 vertices of any job, read
  indirectly. This ends the dispatch-bound cost for small parts.
- **Matrix room that grows.** Size the matrix buffer to the frame's need, or at least 256K matrices (16 MB a
  frame in flight), and warn when anything is drawn unposed.
- **Ray tracing.**
  - Posed characters are not in the traced scene today.
  - If traced shadows or reflections need them later, only the near tier would be refitted each frame.
  - Published advice is to refit from a pose near the current one and to use a coarse LOD for traced copies.

### Networking many agents in co-op

**What it costs now.** At 16 bytes an update and 1,100 bytes a tick, a player can get about 4,000 agent updates
a second. All of that can go to 65 agents at 60 Hz, or be spread thinner.

**What to send.**
- **Smaller updates:**
  - position as a change from the last acknowledged one, in centimetres where it is small;
  - height left to the receiver, which can find it from the ground or the nav;
  - yaw in 8 bits rather than a whole rotation;
  - speed and an animation state in a byte each.
  - That is about 6–8 bytes an agent. A shipped co-op game with large creature counts sends creatures in about
    6 bytes at 5 Hz and found that enough.
- **Rates by tier:**
  - near agents at 20 Hz, mid at 5 Hz, far at 1 Hz, with the receiver smoothing between;
  - the priority that already grows while an agent waits does the rest.
  - At 8 bytes, the budget of 64 KB/s is 8,000 updates a second. For example: 150 near at 20 Hz (3,000),
    600 mid at 5 Hz (3,000) and 2,000 far at 1 Hz (2,000).

**What not to send.**
- **No poses and no animation state beyond a byte.** Each machine animates from movement, as now. Actions
  (an attack, a hit, a death) go as reliable events with their time, so they play in step.
- **Interest by ring.**
  - An agent is sent to a player only if it is in their near or middle ring.
  - Records past that are not sent at all.
  - The far tier's drawn crowds are worked out on each machine from the store's records and seeds. They are the
    same everywhere, because they come from the same saved world, and they become real, sent agents when
    someone comes near.
- **Joining players** get the records of their area with the world's edits, as dimensions' edits are sent
  today.

**What was looked at and set aside.** Lockstep (every machine running every agent from the same commands) carries
huge crowds on tiny bandwidth. It needs the whole simulation to be bit-for-bit the same on every machine, and
players to join from a full copy. Neither fits a physics-driven action game with drop-in co-op.

## Recommendation

**Build in this order:**
1. Box characters drawn from their bones.
2. Skinning that holds to a budget.
3. Importance tiers for agents and animation.
4. Far crowds on baked clips.
5. Records for the streamed world.
6. Smaller network updates.

Each stage stands on its own and shows in a stress scene.

**The targets on an RTX 3070 at 1920×1080, in a release build:**

| Case | Target |
|---|---|
| 5,000 box characters on screen, Mob Motion, along the nav | ≤ 2 ms GPU for posing and drawing them; ≤ 3 ms CPU for animating, stepping and preparing them, spread over threads |
| 200 humanoids near, motion matched, 1,000 in the middle | ≤ 1.5 ms GPU skinning and posing; ≤ 4 ms CPU |
| 10,000 agents in a world of blocks | ≤ 1 ms CPU for the records' ticks; ≤ 1 KB a chunk of records in saves |
| Co-op, four players | Within today's 64 KB/s a player: 150 near at 20 Hz, 600 mid at 5 Hz, 2,000 far at 1 Hz |

**The easy way and the full way.**
- **The easy way:** an Obj with a Nav Agent and Mob Motion, or a Motion Matcher, is in a crowd with no setup.
  Its tier comes from Project Settings ▸ Crowds (distances, rates, budgets) and its distance to players.
- **The full way:**
  - a Crowd Member component's importance and tier overrides;
  - a script's own tier changes (`Crowd.SetTier`);
  - the budgets read and set from C#;
  - every count and timing as Profiler rows, through `vox editor call` and `vox mcp` (`crowd.stats`,
    `crowd.set`), so a tool or an AI can drive it.

## A staged plan

The stages are sized like the roadmap's other parts. Each ends with tests, a manual page and its timings.

1. **Box characters drawn from their bones.**
   - **What changes:**
     - A box model is drawn rigid: the mesh shader poses each vertex by its one bone's matrix, read from a
       frame's matrix buffer by the instance's matrix index.
     - No posed copies and no skinning pass for box models.
     - Matrices are written in parallel, from a pose store rather than a `Pose` component allocated a frame.
     - The matrix buffer grows to fit.
   - **Tests:** a box character posed exactly as skinning posed it (pixels compared), 5,000 in view timed,
     motion vectors right.
   - **Done when:** 5,000 box characters within the target above.
2. **Skinning that holds to a budget.**
   - **What changes:**
     - One dispatch for every job.
     - Only characters seen, or casting a shadow seen, are compute-skinned.
     - Posed vertices get a pool of their own with a size in Project Settings.
     - A character that does not fit is posed where it is drawn.
     - Shared poses for walking and idle characters in the middle distance.
     - A warning and a Profiler row for anything drawn at rest.
   - **Done when:** 200 humanoids compute-skinned and 1,000 more posed where drawn, in a world of blocks, with
     no chunk lighting given back.
3. **Tiers for agents and animation.**
   - **What changes:**
     - Importance and tiers (a crowd resource, the Crowd Member component, Project Settings ▸ Crowds).
     - Rates by tier for brains, steering, avoidance and posing, with the GPU blending between poses.
     - Character Controllers near only.
     - Mob Motion and the Motion Matcher on threads.
     - The Motion Matcher's middle tier: rarer searches, no IK, two clips.
   - **Tests:** an agent changing tier keeps its way and its pose without a pop; 2,000 agents stepped within
     budget.
4. **Far crowds on baked clips.**
   - **What changes:**
     - A model's clips (box clips and Motion Set states) baked into a bone texture as it is imported and
       cooked.
     - The far tier plays them on the GPU from a clip, a start and a speed.
     - The state is picked by speed and the phase by distance walked.
     - Pictures for the furthest humanoids, from the plants' impostor code.
   - **Done when:** 5,000 far characters added for under 1 ms GPU, no CPU animation.
   - **The GPU crowd search** (M6) fits here or after: middle-tier searches for hundreds of characters at once.
5. **Agents in the streamed world.**
   - **What changes:**
     - Records by chunk.
     - Objs to records and back as rings move, a few a frame.
     - Far records walking the region graph.
     - Shared direction fields for many agents to one goal.
     - Records in saves, compact.
     - Ambient life spawned from seeds.
   - **Tests:** a crowd walks out of the rings and back and is where it should be; a save keeps them; 10,000
     records ticked within budget.
6. **Networked crowds.**
   - **What changes:**
     - Compact agent updates: changes from acknowledged positions, yaw, speed and state.
     - Rates by tier per player.
     - Interest by ring.
     - Actions as timed reliable events.
     - Far crowds worked out on each machine from records and seeds.
     - Records sent to joining players.
   - **Tests:** four simulated players with 2,000 agents each in view, within 64 KB/s, through the network
     simulator's loss and latency.
7. **A crowd stress scene and the vox calls.** This runs alongside the others rather than after them:
   - a sample of a town, a field and a dungeon with crowds;
   - `crowd.stats` and `crowd.set` through the editor link and MCP;
   - the Profiler rows.

   Each stage adds its part of the sample, and its timings are measured there.

**Where to start.** Stages 1 and 3 give the most for the game soon: its block worlds are full of box characters,
and every agent costs the same everywhere today. Stage 2 matters when humanoid characters arrive in numbers.
Stage 6 is needed before crowds are tried in co-op.

## References

Techniques and figures above come from these. They are listed for reading, not as names the engine takes.

- Skinned instancing white paper (2007): https://developer.download.nvidia.com/SDK/10/direct3d/Source/SkinnedInstancing/doc/SkinnedInstancingWhitePaper.pdf
- Animated crowd rendering, GPU Gems 3 ch. 2: https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-2-animated-crowd-rendering
- A GPU crowd of 65,000 agents (SIGGRAPH 2008): https://www.advances.realtimerendering.com/s2008/SIGGRAPH2008%20-%20March%20of%20the%20Froblins.pdf
- High zombie throughput in modern graphics (GDC 2019): https://gpuopen.com/gdc-presentations/2019/gdc-2019-agtd4-high-zombie-throughput-in-modern-graphics.pdf
- Learned motion matching (2020): https://www.theorangeduck.com/media/uploads/other_stuff/Learned_Motion_Matching.pdf
- High-performance animation (SIGGRAPH 2017): https://history.siggraph.org/wp-content/uploads/2022/09/2017-Talks-Bollo_High-Performance-Animation-in-Gears-of-War-4.pdf
- GPU-based motion matching for crowds (poster): https://scholar.usuhs.edu/en/publications/gpu-based-motion-matching-for-crowds-in-the-unreal-engine/
- Massive crowds by recycling AI (GDC 2015): https://www.gdcvault.com/play/1022141/Massive-Crowd-on-Assassin-s
- A write-up of that talk with its numbers: https://game.watch.impress.co.jp/docs/news/20150307_691715.html
- Crowd pathfinding and steering using flow field tiles: http://www.gameaipro.com/GameAIPro/GameAIPro_Chapter23_Crowd_Pathfinding_and_Steering_Using_Flow_Field_Tiles.pdf
- Optimal reciprocal collision avoidance: https://gamma.cs.unc.edu/ORCA/
- Local avoidance at 10,000 agents: https://arongranberg.com/astar/docs_beta/local-avoidance.html
- Animation held to a budget: https://dev.epicgames.com/documentation/en-us/unreal-engine/animation-budget-allocator-in-unreal-engine
- Sharing poses among a crowd: https://dev.epicgames.com/documentation/unreal-engine/animation-sharing-plugin-in-unreal-engine
- Skinned mesh rendering paths and the skin cache: https://dev.epicgames.com/documentation/en-us/unreal-engine/skeletal-mesh-rendering-paths-in-unreal-engine
- Posed vertices for ray tracing: https://masonstevenson-dev.github.io/posts/2024-03-12-SkinCache
- Baking vertex animation: https://dev.epicgames.com/documentation/unreal-engine/vertex-animation-tool-in-unreal-engine
- Replication by distance for many entities: https://dev.epicgames.com/documentation/en-us/unreal-engine/overview-of-mass-gameplay-in-unreal-engine
- 6,000 agents, baked against skinned, on an RTX 3070: https://80.lv/articles/6-000-agents-on-one-game-thread-novel-high-performance-framework-for-gpu-driven-crowd-systems/
- Ray tracing best practices (refitting moving meshes): https://developer.nvidia.com/blog/best-practices-for-using-nvidia-rtx-ray-tracing-updated/
- Snapshot compression: https://gafferongames.com/post/snapshot_compression/
- State synchronization and priority accumulators: https://gafferongames.com/post/state_synchronization/
- Lockstep for 1,500 units on a modem: https://www.gamedeveloper.com/programming/1500-archers-on-a-28-8-network-programming-in-age-of-empires-and-beyond
- Prioritised replication in a 16-player shooter (summary): https://ng.cs.ucl.ac.uk/?p=115
- Creature movement in a co-op game at 5 Hz: https://www.exorstudios.com/blog/navmesh
- 400 enemies across four co-op players: https://coherence.io/blog/tradecraft/vampire-survivors-online-coop-case-study
- 1,000+ networked NPCs by move segments: https://d34kmbgkaszffl.cloudfront.net/blog/adding-more-than-1000-npcs-in-multiplayer-without-network-saturation-in-unreal-engine
