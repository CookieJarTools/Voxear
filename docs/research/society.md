# A living society: residents with routines in streamed worlds

Research for running a simulated society in Voxear's endless, streamed worlds of blocks, written 2026-11-21: many
residents with daily routines (home, work, shop, tavern, bed), who keep living while their chunks are unloaded and
are where they should be when a player comes back. Nothing here is built yet. It covers what the engine has now
(measured where it could be), the options, a recommendation with target numbers, and a staged plan split between
the engine and the game (CubeGame).

It builds on two earlier plans and does not repeat them:
- [crowds.md](crowds.md): tiers by distance, the Crowd Member component, crowd records, far crowds drawn from baked
  clips, compact network updates. All built.
- [world-building.md](world-building.md): world layers, Ground Fit, Streams With Ground and column events (built),
  Roads and Area Fill (built), the Placement component (stage 3), World Areas and Routes (stage 5).

## The short version

- **Most of a society is never simulated, only worked out.** A resident's day is data: blocks of hours, each with
  a place and an activity. Where a resident is at any moment follows from the schedule, its places and the time,
  with travel as distance over speed along a known way. Working that out took **9 ns a resident** in a prototype,
  so a town of 300 is placed in 3 µs when a player walks in, and 100,000 in under 1 ms.
- **The engine's crowd records are the right base, but they only move.** They keep a prefab, a position, a
  straight way and a speed. They keep no schedule, no home and no state, and a resident made again from a record
  is a fresh prefab. They walk in straight lines through unmade ground and are all walked once a second, however
  far off.
- **What is missing, in order:** a world time that counts days; schedules and places as typed data; places known
  while their ground is unloaded; records that hold a resident's day and wake in the right spot; then needs,
  reactions and a coarse society tick for the economy.
- **Co-op comes nearly free.** Schedules are pure functions of the world time and saved data. Host and players
  work them out alike, so nothing is sent for residents nobody is near. Only changes to the plan (an override, a
  death, a purchase) go over the network, as events.
- **Saves stay small.** Residents of generated towns come from the town's seed. Only what differs from that is
  saved: about 3 bytes a resident on average in a prototype where one in ten has anything of its own.

**Numbers to aim for** (RTX 3070 class desktop, release build):

| What | Count | Budget |
|---|---|---|
| Residents the world knows | 100,000 (tested to 1,000,000) | ≤ 8 MB in memory; ≤ 1 ms CPU an in-game hour for the society tick, off the main thread |
| Drawn as far ghosts | 2,000 (today's `MOST_DRAWN`) | as crowds.md's far tier |
| Present as Objs near players | 300–500 a player in a town, 1,000 in a city | as crowds.md's near and middle tiers |
| Woken or put away a frame | 16–32 | ≤ 0.3 ms a frame on the main thread |
| Saved | 0 bytes for a resident as its seed made it | ≤ 300 KB for 100,000 residents of which one in ten is changed |
| Network for residents no one is near | 0 | events only |

## What the engine has now

### Crowd records (`voxear_engine::crowd_records`)

- **What becomes a record.** A root Obj made from a prefab, in the level's dimension, with a Crowd Member marked
  **Keep as Record**, once it is further than Project Settings ▸ Crowds ▸ **Records beyond** (250 m) from every
  viewpoint. It comes back as an Obj within that distance less 30 m, at most 16 changes a frame.
- **What a record keeps** (`CrowdRecord`): an id, the prefab, a position from the level's real middle, a yaw, a
  destination, the corners of its nav way not yet passed, and a speed. Nothing else: no script fields, no
  schedule, no home. The Obj made again is the prefab, sent on to where the record was going.
- **How it moves.** Once a second every record walks on along its corners, then straight to its destination,
  "with no ground under it". When it gets there it stands.
- **Where they are kept.** One `Vec` for the level. Every frame `update` measures every record against every
  viewpoint to find those to make again.
- **Saves and C#.** `CrowdRecords.Capture` and `CrowdRecords.Apply` in C#, a varint layout (`VXCR\x02`): each
  prefab once, positions to a centimetre, each way corner from the one before.
- **Drawing far off.** Records within **Draw records within** (600 m), the nearest 2,000, are drawn as ghosts
  playing their prefab's walk and stand clips, smoothed between the records' one-second steps.
- **Co-op** (`net::crowd_sync`). The server sends each player the records within drawing distance, as bytes, when
  records are made or woken and every 5 s besides. Players walk them on between and never make any.
- **Only the level's dimension** has records. A dungeon's characters are not kept.

### Measured

The record code was copied as it is (`walk`, `to_bytes`, and the nearness scan in `update`) into a small
benchmark, so it could be timed at larger counts without building the engine. Records had four way corners and a
destination, spread over 20 km, with four viewpoints. Least of 20 runs.

| Records | Walk, once a second | Nearness scan, every frame | Save (`to_bytes`) | Saved a record |
|---|---|---|---|---|
| 10,000 | 0.14 ms | 0.07 ms | 1.2 ms | 41 B |
| 50,000 | 0.71 ms | 0.31 ms | 5.8 ms | 42 B |
| 100,000 | 1.5 ms | 0.56 ms | 11.6 ms | 42 B |
| 250,000 | 3.8 ms | 2.3 ms | 30 ms | 42 B |

At `opt-level = 1`, as test builds have it, the scan is about 1.7 times as long and walking about 1.2 times.
A record standing still saves in 15 bytes. In memory a record with four corners takes about 200 bytes, mostly its
way's own allocation.

**What the numbers say:**

- **Ten thousand records are cheap; a hundred thousand are not free.** The walk and the scan grow with the count,
  whether anyone is near or not. A society of 100,000 would spend about 1 ms every frame and 1.5 ms every second
  on characters nobody can see. Kept by area and woken only near, the cost would grow with what is near instead.
- **Saved size is mostly the way.** A point to a centimetre from the world's middle is 3 varints of 3–4 bytes.
  A resident on a known route needs only the route and the distance along it: a few bytes.
- **The prefab lookup in `to_bytes` is linear** in the prefabs, which does not matter at 8 prefabs. Hundreds would
  want a map.

The engine's own crowd tests (`crates/voxear_engine/tests/crowds.rs`) cover records walking out and back and
being drawn. They were not run here, as nothing in them changed.

### Schedules, prototyped

A resident with a home, a workplace and a tavern, on a six-block day (eat at home 6:00, work 7:30, lunch 12:00,
work 13:00, tavern 18:00, home to bed 21:30), placed at any hour from the schedule alone: the block for the hour,
the place before and the place now, and how far it has walked from one to the other at 1.4 m/s on a 20-minute
day.

| Residents | Where each is now | An hourly step of needs and money |
|---|---|---|
| 10,000 | 0.09 ms (9 ns each) | 0.02 ms |
| 100,000 | 0.9 ms | 0.16 ms |
| 1,000,000 | 9.1 ms | 1.6 ms |

That is in a release build; `opt-level = 1` is about twice as long. Straight lines stand in for routes; a lookup
along a route's points by distance adds a binary search, still well under a microsecond.

### Other parts a society stands on

- **The clock** (`heat::Climate`, `Clock` in C#) keeps the hour of the day as an `f32` that wraps at 24, turning a
  day every Day Length minutes (20 by default), per dimension or shared with the level. **It does not count
  days**, so "Tuesday", "three days later" or "how long was I away" cannot be asked. Setting `Clock.TimeOfDay`
  jumps the hour (the example given is sleeping till morning) and tells no one.
- **Streams With Ground** (`ground_streaming`) takes a house and everything in it out as a kept scene when its
  column goes and makes it again exactly when it comes back, telling behaviours with `OnGroundUnloading` and
  `OnGroundLoaded`. `GroundMoves` lists each frame's columns made and let go (`blocks.columns`).
- **Navigation in worlds of blocks** is made per chunk as chunks stream in, with regions and a coarse graph. It
  exists only for made chunks, so far residents cannot use it.
- **Roads** (`voxear_engine::roads`) are authored splines with `nearest` queries; **generated towns**
  (`voxear_terrain::blocks::structure::Town`) lay out their middle, streets and houses from the seed, each house
  turned to face its street with its door at a known side, and their roads to neighbouring towns, worked out from
  the two towns alone and cached (`road_between`). Both can be read without making a chunk.
- **Crowd tiers** (near 40 m, middle 150 m, far beyond) set how often each Obj plans, moves and is posed.
- **Planned, not built:** the **Placement** component (footprint, front, door as child Obj references) in
  world-building stage 3; **World Areas** with tags and **Routes** with stops and `Routes.Find` in stage 5.

### Limits, together

1. No world time with days, and time jumps tell nothing.
2. A record keeps movement only. What makes a resident (its day, its places, its state) is lost while far.
3. Records walk straight through unmade ground and stop at a destination: they do not follow roads or plans.
4. Every record is walked and measured every second and frame, near or far.
5. Places (homes, beds, counters) are not data. A house unloaded with its ground cannot be found or claimed.
6. A record woken comes back at its walking point, never in a bed or behind a counter, and before or after its
   house is restored depending on frame order.
7. Records in co-op go over the network as whole lists every 5 s, though the server and the player could work
   them out alike.
8. Only the level's dimension keeps records.

## The options

### Representing a resident while no one is near

**Step it, at a coarse rate.** Tick each far resident every in-game hour: advance its activity, move it along its
way by speed times the hour. Simple, and anything can happen at each step. The cost grows with the population
whether anyone is near, and every step is state to save and, in co-op, to agree on.

**Work it out when asked (recommended).** Keep only what does not follow from the schedule: the schedule, the
places, the seed, and any changes to the plan. Ask "where is it now" only when it matters: a player comes within
waking distance, a ghost is drawn, a script asks, a delivery arrives. The answer is a pure function of world time.
This is how published work handles pedestrians in open worlds: start an agent with the least information and
make up the rest on demand, consistent with what the player has already seen. It also makes host and players agree
without sending anything, and an editor can show where everyone would be at 3 a.m. by scrubbing a slider.

What it cannot do on its own is history: a resident that should have been robbed on the road, or a shop that ran
out. Those come from a **coarse society tick** that changes the saved data (stock, money, deaths, overrides), not
from stepping each resident. Between ticks, everything is still worked out.

**Learned or statistical stand-ins** (predicting outcomes instead of simulating, or keeping only probabilities a
far area's residents follow) suit outcomes nobody checks against a single resident. They are worth keeping in mind
for far battles or a town's mood, not for where the baker is.

### Schedules as data

- **A schedule** is a list of blocks: from an hour, an activity, a place by slot (Home, Work, a Tavern, the
  nearest Shop of a kind, a Route Stop), and optionally which days of a week. Shared as an asset by every resident
  of a kind; a resident adds its own places, its seed and its walking speed.
- **Variety without data** comes from the seed: each resident's blocks shift by a few minutes, a day off on a
  hashed day, a hashed choice between two taverns. Still pure functions of the seed and the date.
- **Travel** starts so as to arrive: the leg from the last place to the next begins at the block's hour less the
  way's length over the speed, or at the hour itself when the schedule says "leave at". A leg's way is a Road or
  Route (stage 5), a generated town's streets and roads, or a straight line when there is nothing better.
- **Activities** are assets: what is done at the spot (animation set, how long, interruptible or not, what it
  satisfies). Far off, an activity is only its name and its spot.

Published work on cyclic scheduling builds believable days the same way, from repeating blocks with constraints,
and places that offer behaviours to whoever enters them ("smart areas", "smart objects") keep the schedule short:
the schedule says "the tavern", the tavern says "sit, drink, talk".

### Deciding what to do

| Way | What it does | Fit here |
|---|---|---|
| **Schedule with interrupts** | Follows the day; something urgent (danger, a call from the player, a need past a threshold) pushes a short override with an end time, then the day resumes where the clock says. | **The base.** Works out in closed form; overrides are small data. |
| **Utility scoring** | Each choice scored by curves of inputs (hunger, money, distance, hour) multiplied together; the highest wins. Adding a consideration does not disturb the others; a few preset curves cover most cases. | **Choices inside the schedule** (which shop, whether to eat now) and reactions. Cheap, tunable in data. |
| **Behaviour trees** | A tree of sequences and fallbacks ticked each frame. | **Activities at full detail only**, near the player: the steps of serving at a counter. Places can hand trees to whoever arrives. |
| **Goal planning** | Searches for a sequence of actions whose effects reach a goal. | Set aside for society: a search a decision, hard to work out in closed form, and its variety is not needed for routines. Possible later for combat. |

**Needs** (hunger, sleep, money, mood) are kept as a value and the time it was set, rising or falling at a known
rate. Their value at any time is worked out, and schedule blocks that satisfy them (meals, bed) reset them at known
hours, so needs stay in closed form too. A need only acts on the plan when it crosses a threshold, which is an
interrupt.

### Handing over between record and Obj

A resident has three states, extending crowds.md's tiers:

| State | Where | What it is | What it costs |
|---|---|---|---|
| **Present** | Within Records beyond (250 m) of a player | An Obj: Nav Agent, animation, behaviours, the crowd tiers near/middle/far | As crowds.md |
| **Ghost** | Out to Draw records within (600 m), on screen | A record moved along its leg, drawn from baked clips | As crowds.md's far tier |
| **Dormant** | Everywhere else | Data only, never ticked | Nothing until asked |

**Waking** (dormant or ghost to present), for each resident in a cell coming within waking distance:
1. Wait until the ground there is made, its kept Objs (the house and its furniture) are made again, and the nav
   is ready. Residents come after their buildings, never before.
2. Work out its activity, place and progress now.
3. At a place: put it at its spot (its bed, its counter, its stool), in the activity's pose, already doing it.
   **At night, residents are in their beds when a player walks in.**
4. On a leg: put it at the point along the way, set onto the nearest nav place, sent on to the rest of the leg.
5. If it was a ghost, swap at the same position, facing and clip phase, as crowd records already do.

Waking at 250 m with 16 a frame wakes a town of 300 in under 20 frames, well before a player at walking pace sees
its streets up close. Residents indoors are out of sight anyway.

**Putting away** (present to record), before its ground goes (`OnGroundUnloading`) or past Records beyond:
1. Note whatever differs from the schedule: an override and its end, needs, money, carried items, opinions, by
   the fields its scripts mark as kept.
2. Free its claims on spots it was only using (a seat), not those it owns (its bed).
3. Its Obj goes. Its position is not kept: it follows from the schedule again.

**Time jumps.** Sleeping, waiting or a script setting the clock: the world time moves on, and every present
resident out of every player's sight is placed again as if woken; those in sight walk to where they should be.
Dormant residents need nothing.

**Keeping what the player saw.** If a player watched a resident leave the tavern late, the resident should not be
found back at the tavern. An override recorded when a resident leaves its plan in view (it was talked to, it fled)
keeps it consistent; otherwise the schedule holds.

### Places

- **A Building** is an Obj (a placed prefab, or a generated house) with a kind: Home, Shop (of a kind), Workplace
  (with jobs), Tavern, Hall. Kinds are typed: an enum for the built-in ones, a Building Kind asset for the game's.
- **Spots** are named points on its prefab: bed, door, counter, stall, seat, workbench, oven. Each is a child Obj
  marked with a Spot component (kind, the activity it offers, how many at once). The planned **Placement**
  component already names a door as a child Obj reference; Spots extend it rather than adding another list.
- **References are typed.** A resident's Home is a picker over Buildings, never a name. Far off, a place is a
  `PlaceRef`: the building's id (a scene Obj's id, or a generated town's site and house number) and a spot index.
- **Claims.** A spot is owned (a bed, a shop's counter) or taken for a while (a seat). Claims are kept in the
  society, not on the Objs, as the Objs unload. A claim on a spot whose building is destroyed is freed and the
  resident finds another or becomes homeless.
- **Places known while unloaded.** The society keeps an index of buildings and their spots in world positions:
  from scene Objs as they are loaded or kept by Streams With Ground, and for generated towns from the town's layout,
  which is already worked out from the seed without making chunks. A house structure gains a Building with its
  spots in its own space.

### A society

- **Populating generated towns.** A town's houses each say how many residents of which kinds live there (a cottage
  2–4, a smithy one smith and family). Residents are made from the town's seed when the town first comes within
  the society's reach, and kept only once they differ. A town never visited has residents only as a rule.
- **An economy kept by settlement, not by resident.** A town keeps stock of goods by typed item reference (the
  inventory package's items) and money. The society tick (an in-game hour, or a day) adds what its workplaces make
  and takes what its residents use; shops sell from the town's stock. Prices follow supply and demand, as in
  published work where traders change trade when theirs stops paying.
- **Deliveries** are residents with a trading schedule between two settlements along a Route: a carter, a ship.
  Their arrival moves goods between ledgers at a known time. Waylaid by a player, the delivery does not arrive and
  the town runs short. This is where the economy and the routes meet.
- **Relationships, kept sparse.** Opinions exist only when not the default: a settlement's standing with each
  player, a faction's, and a resident's own only after it has met the player. A small record a pair.
- **Reactions and events.** An event (a fight, a fire, a theft, a dragon) has a place, a radius, a kind and an end.
  Present residents react by utility (flee, gather, fetch the guard). Far off, the event changes a settlement's
  data (fear, standing, stock) at the society tick. Witnesses exist only present.

### Determinism and co-op

- **World time as an integer.** A count of in-game seconds (`u64`) from the world's start, kept by the server and
  sent to players, gives the day, the week and the hour. Schedule evaluation reads it and the saved data only, with
  hashes of whole numbers for the seed's variety (the engine already learnt that hashes of `sin` break on the GPU;
  on the CPU the same rule keeps machines in step).
- **Exactly the same is only needed for decisions,** not for positions: which block, which place, which shop. Those
  come from integers and hashes. Positions along a leg are floating point and may differ by a hair on two machines,
  which only shows on ghosts; present residents are the server's Objs, sent as now.
- **What goes over the network:** the world time; society events (an override, a death, a claim, a purchase, a
  delivery arriving) as reliable messages; present residents as Objs at crowds.md's rates. Not the dormant or ghost
  residents: each machine works them out. This replaces the whole-list record sends every 5 s.
- **Who simulates.** The server (the host in co-op) owns the society: the tick, claims, overrides. Players far
  apart each get their own surroundings woken; the society tick is global and does not care where players are.
  Lockstep of the whole society was set aside, as in crowds.md: it needs every machine to step everything
  identically and joins from a full copy.
- **Joining players** get the society's changes with the world's edits, as they get dimensions' edits now.

### Persistence

**Saved:**
- the world time;
- for each settlement: stock, money, standing, and which of its seed's residents differ;
- for each changed resident: its id, then only the fields that differ, each as a varint or a byte: an override and
  its end, needs (a byte each), money, a different home or job, dead, carried items, opinions, script fields marked
  kept;
- placed residents (put in a scene by hand), in the scene as now;
- claims that differ from the defaults.

**Not saved:** positions, legs, activities, ghosts. They follow from the rest.

In the prototype, 100,000 residents of which one in ten had money, four needs and an opinion of their own saved in
270 KB, 2.8 bytes a resident. A town untouched saves nothing.

### Scale and budgets

- **Memory.** A dormant resident is an id, a schedule index, three or four place references, a seed and a speed:
  about 32 bytes, plus its changes. 100,000 are about 4 MB; the index of places a few MB more.
- **CPU, dormant.** Nothing a frame. The society tick, an in-game hour (3 real seconds on a 20-minute day), runs
  per settlement on the terrain's threads: 100,000 residents' needs and money in about 0.2 ms, with the game's
  hooks per settlement, not per resident.
- **CPU, waking.** 16–32 a frame at a few µs each to work out, plus making the Obj, which is the real cost (the
  prefab's components) and stays spread over frames.
- **CPU, present.** As crowds.md: 200 near and 1,000 middle per player. A schedule-following resident adds a
  check at each block's hour and a nav request a leg, which is less than an idle Nav Agent already costs.
- **Keeping records by area.** The records move from one list to cells of the world (say 128 m), so waking and
  drawing look only at cells near players. That ends the per-frame cost of 0.56 ms per 100,000 records far off.

### Scripting

**The easy way:** add a **Resident** component to a prefab, pick its **Schedule** asset, and pick its Home and
Work buildings in the Inspector (or leave them for its town to fill). It walks to work at eight and home at night,
sleeps in its bed, and does the same while unloaded. Schedules are edited as a timeline of blocks in the Inspector.

**The full way:**
- `Resident` in C#: `Schedule`, `Home`, `Work`, `Activity`, `Place`, `Needs`, `Override(activity, place, until)`,
  `Resume()`, and `[Kept]` on a script's fields to keep them in the record.
- Hooks on a resident's behaviour while it is present: `OnActivityStarted`, `OnChooseActivity` (return a choice to
  replace the schedule's), `OnEvent`.
- `Society` for the data far off: `Society.Time`, `Society.Residents.Near(point, radius)`, `Society.Find(id)`,
  `Society.WhereAt(resident, time)`, `Places.Find(kind, near)`, `Places.Claim`, settlements' stock and money, and
  `Society.OnTick` hooks per settlement for the economy.
- `vox editor call` and `vox mcp`: `society.stats` (counts by state, the last tick's time), `society.residents`
  (near a point, with each one's activity and place), `society.resident.get` and `.set`, `society.time` (read, set,
  skip), `society.places`, `society.preview` (where everyone would be at a given time). Components and assets go
  through `component.add`, `field.set` and the asset calls as now.

### Editor

- A **Society** view: the residents of the selection's town, each one's day as a bar of blocks, and a time slider
  that moves ghosts to where everyone would be at that hour, without playing. Worked out, so it is instant.
- Gizmos for spots on a prefab, and lines from a resident to its home and work.
- Profiler rows: present, ghosts, dormant, woken and put away this frame, the tick's time.

## Recommendation

**Schedules with interrupts, worked out on demand, with a coarse society tick for what changes.** Build on the
crowd records rather than beside them: a resident record is a crowd record with a schedule, places and changes,
kept by area, and its position is no longer stored but worked out. Places come from Buildings and Spots on
prefabs (through the planned Placement component) and from generated towns' layouts. Utility scoring for choices
and reactions, behaviour trees only for activities seen up close. The server owns the society; every machine works
out the same plan from the world time.

**Why not tick everyone coarsely:** it costs with the population, every step is state to save and send, and it
gives nothing a player can see that working it out does not.

## A staged plan

Each stage lands on master with its tests, a manual page and its timings, as the roadmap's others do.

| Stage | Contents | Whose | Size |
|---|---|---|---|
| 1. World time and schedules | A world time counting days (`u64` seconds, the Clock reads from it, day and week in C#), time-jump events; **Schedule** and **Activity** assets; **Building** and **Spot** components (on Placement once it lands, standing alone till then), claims; the **Resident** component following its schedule while present: walking to its place, using its spot; `Society.WhereAt`; the Inspector's schedule timeline; `society.time` and `society.preview`. Test: a shopkeeper opens the shop at eight and is in bed at midnight; a time skip puts everyone right. | Engine | Medium |
| 2. Residents while unloaded | Resident records from crowd records: kept by cell, holding schedule, places, seed and changes, positions worked out; dormant, ghost and present; waking after the ground and its kept Objs, at spots in their activity's pose, or along a leg; putting away before the ground goes; legs along Roads and generated towns' streets and roads, straight otherwise; `[Kept]` script fields; compact saves; records in dimensions. Test: leave a town at 20:00, come back at 03:00, everyone is in bed and nothing pops; 100,000 dormant records cost nothing a frame. | Engine | Large |
| 3. Places without loading, and towns that people themselves | The place index from scene Objs (with Streams With Ground's kept ones) and generated towns' layouts; a Building and spots on house structures; a Population rule on towns making residents from the seed, saved only once changed; homes, jobs and beds handed out. Test: walk into a generated town never visited and find its smith at the forge. | Engine | Medium |
| 4. Needs, interrupts and reactions | Needs in closed form with thresholds; overrides with an end; utility scoring with preset curves, in data; events with place, radius and kind; standing with settlements and factions, sparse opinions; C# hooks (`OnChooseActivity`, `OnEvent`). Test: a fight in the square empties it; the shopkeeper comes back after. | Engine framework, the game's content | Medium |
| 5. Society tick and economy | The tick per settlement on threads with `Society.OnTick`; settlement stock and money by typed items from the inventory package; deliveries as travelling residents on Routes whose arrival moves goods. CubeGame's goods, prices, jobs and trade rules. Needs world-building stage 5 (Routes). | Engine tick and ledgers; the economy is the game's | Medium |
| 6. Co-op | The world time sent and kept by the server; society events as reliable messages; players working out dormant and ghost residents themselves, ending the record sends; joining players given the society's changes. Test: four simulated players, two far apart, see the same residents in the same places, within 1 KB/s of society traffic when no one interacts. | Engine | Medium |
| 7. Tools and a sample (alongside) | The Society view with its time slider, spot gizmos, Profiler rows, `society.*` calls in the editor link and MCP; a sample town with a day of routines. | Engine | Small, spread over the stages |

**What is CubeGame's:** the schedules and activities themselves, building prefabs with their spots, the town
population rules, jobs, goods, prices and trade, dialogue, crimes and guards, factions and what residents say about
the player. The engine provides the data types, the working out, the handover, the tick and the hooks.

**Order and dependencies.**
- Stages 1 and 2 are the user's question: routines that go on while unloaded and are right when you return. They
  need nothing unbuilt.
- Stage 3 is better after world-building stage 3 (Placement), but can put Spots on prefabs first and move them.
- Stage 5 needs world-building stage 5 (Routes) for long trips; until then, legs follow Roads and generated roads.
- Stage 6 can come any time after 2, and before society is tried in co-op.

**Where to start:** stages 1 and 2 together give a town whose people keep their day, wake in their beds and cost
nothing while far.

## Building it in the game instead

Decided 2026-11-22: the society is built by the game in C#, on the engine, rather than into the engine. The stages
above stay as the plan, but nearly all of them move into CubeGame. The engine gives four hooks the game cannot make
itself. Everything else the game already has.

### What the game builds with what is there

| Part | With |
|---|---|
| Schedules, activities, needs, settlements, the economy | Plain C# data and assets of the game's own types, picked by typed fields |
| World time with days | `Clock.TimeOfDay`, counting a day each time it wraps; time skips go through the game's own code, which sets the clock |
| Residents while unloaded | An array of structs in a society class; positions worked out from the schedule, never stored |
| Waking hand-placed houses' residents | A behaviour on each house prefab, which has Streams With Ground: `OnGroundLoaded` comes once the house and its furniture are back, and spawns its residents at their spots (`Prefab.Instantiate`). `OnGroundUnloading` notes what changed and destroys them |
| Spots and claims | A Spot component of the game's own on child Objs of the prefab; claims kept in the society |
| Present residents | Nav Agents and the crowd tiers, as any character |
| Travel | `Roads.Nearest` and road points; straight lines elsewhere until Routes land |
| Saves | `Save` and `SaveFile`, writing only what differs from each resident's seed |
| Co-op | Voxear.Networking: the host owns the society and sends its events as RPCs; players work out schedules themselves from the clock |

### The four hooks

**Built** (2026-11-22): hooks 3 and 1, as column events (`Blocks.ColumnMade`, `Blocks.ColumnLetGo`, one event a
column) and `FarFigures`. The manual's Streams With Ground and Crowds pages show them.

**1. Far figures: ghosts drawn from the game's positions.**
- **Why.** Past Records beyond (250 m) a resident has no Obj, and only the engine's crowd records are drawn far off.
  C# can only save and restore those as bytes, and the engine moves them, not the game.
- **What.** A second source for the ghosts beside the records: a list the game sets of figures, each an id, a
  prefab, a position, a yaw and a speed (0 standing). `crowd_records::to_draw` takes the nearest of both, within
  Draw records within and up to `MOST_DRAWN`. `move_ghost` already eases a ghost towards where it should be and
  plays walk or stand by speed, so figures set once a second move smoothly.
- **C#.** `FarFigures.Set(ReadOnlySpan<FarFigure>)`, replacing the list, and `FarFigures.Clear()`. A figure's id
  ties it to its ghost from one set to the next, so the game can send only figures near players, a few thousand.
- **Co-op.** Each machine sets its own figures from the society it works out, so nothing is sent.
- **Tests:** a figure is drawn with its prefab's model and clip; set again further on, the ghost walks there rather
  than jumping; a figure dropped from the list goes.
- **Size:** small.

**2. Generated towns' plans.**
- **Why.** A generated town's houses are blocks made with its chunks, not Objs, so nothing tells the game a house
  is there, which kind it is or where its door is. `Town::layout` works all of it out from the seed alone, but it
  is private, and its parts do not keep which house kind they were.
- **What.** Parts keep their house kind. A query gives the towns whose sites are within a distance: each town's
  structure, its middle, and its houses, each with its structure, where it stands, which way it faces and where its
  door is. Worked out from the generator with no chunk made, as the town's own roads are, and cached per site.
  Points inside a house (beds, a counter) come from the house structure's own data, which gains named points in
  its model's space.
- **C#.** `Towns.Near(Vector3 point, float radius)` giving `TownPlan`s, with `Houses` as `TownHouse` (a
  `Structure` reference, `Position`, `Facing`, `Door`, `Points`). `vox editor call towns.near` gives the same.
- **Tests:** a town's plan matches the houses its chunks are built with, from any column and before any is made.
- **Size:** medium.

**3. Column events in C#.**
- **Why.** Residents of generated houses, and residents on a road, have no house Obj to hear `OnGroundLoaded`
  from. The engine keeps each frame's columns made and let go (`GroundMoves`) for `vox editor call blocks.columns`,
  but C# cannot read them.
- **What.** `Blocks.ColumnMade` and `Blocks.ColumnLetGo` events, one a column, with its place among the world's
  32-block columns, its corner and its size, and `Blocks.IsMade(Vector3)`. Made means the blocks are there. A column's nav may come a few frames
  later, so `Navigation.IsReady(Vector3)` says when an agent can be sent from there.
- **Tests:** a script hears a column made as the camera nears it and let go as it leaves, and the same columns as
  `blocks.columns` lists.
- **Size:** small.

**4. The game's own `vox editor call` and `vox mcp` calls.**
- **What is there.** A `[Command]` method with arguments runs from `vox editor call game.console` while the game
  plays, as a console line, and gives back what it logged. Palette commands run with `command.run`, without
  arguments.
- **Why that is not enough.** The answers are log lines, not data a tool can read. A tool cannot list the calls
  with their arguments. Nothing runs while the editor is not playing, as a schedule preview would need.
- **What.** A `[Call("society.residents")]` attribute on a static C# method. Its arguments are typed (numbers,
  text, enums, vectors, asset and Obj references) and its return value is any value `Save` can write, given back as
  JSON. The link lists such calls beside its own in `calls`, with their arguments and the method's summary, so
  `vox mcp` offers them as tools. Each says whether it runs in the editor, while playing, or both.
- **Tests:** a project's call is listed, is called with arguments and gives back its value; called while not
  playing, a playing-only call says so.
- **Size:** medium.

### What moves where

| Stage above | Now |
|---|---|
| 1. World time and schedules | The game's, on the clock. Engine: nothing. |
| 2. Residents while unloaded | The game's. Engine: hooks 1 and 3. |
| 3. Places without loading, populations | The game's. Engine: hook 2. |
| 4. Needs, interrupts, reactions | The game's. |
| 5. Society tick and economy | The game's, on the inventory package. Engine: Routes, as world-building stage 5 plans. |
| 6. Co-op | The game's, over Voxear.Networking. |
| 7. Tools | The game's own calls through hook 4; a schedule view as a game editor window. |

**Order:** hooks 3 and 1 first, as residents while unloaded need them, then 4 so the society can be driven by
tools from the start, then 2 when generated towns are peopled. Each lands on master with its tests and a manual
page.

## References

Techniques and figures above come from these. They are listed for reading, not as names the engine takes.

- Perceptually realistic behaviour through alibi generation (2010): https://core.ac.uk/download/76364699.pdf
- Managing simulation level of detail with the LOD Trader (2013): https://researchr.org/publication/Sunshine-Hill13/bibtex
- Simulation level of detail for virtual humans (2007): https://core.ac.uk/works/16904818
- Level of detail for character AI, navigation, smart objects and dialogue: https://opus.bibliothek.uni-augsburg.de/opus4/frontdoor/deliver/index/docId/46028/file/46028.pdf
- Supporting thousands of NPCs with routines in an open world (GDC): https://gdcvault.com/play/1035718/Supporting-Thousands-of-NPCs-in
- Predicting outcomes instead of simulating them (2016 dissertation): https://publications.scss.tcd.ie/theses/diss/2016/TCD-SCSS-DISSERTATION-2016-055.pdf
- Smart Areas: daily life in open worlds (2014): https://scitepress.org/PublishedPapers/2014/49211/pdf/index.html
- Cyclic scheduling for believable behaviour (2014): https://cspages.ucalgary.ca/~richard.zhao1/publications/2014aiide-Cyclic-Scheduling.pdf
- Systems for non-combat AI, with a scheduling system (GDC 2019): https://gdcvault.com/play/1025833/Stop-Fighting-Systems-for-Non
- Ambient AI and simple utility for living places (GDC 2012): https://gamedeveloper.com/programming/gdc-2012-i-sims-i-team-s-graham-says-ambient-ai-tells-better-stories
- Smart objects for town characters (GDC 2018): https://gamedeveloper.com/programming/the-ai-systems-that-govern-i-final-fantasy-xv-i-s-believable-npcs
- Utility systems, considerations and response curves: https://en.wikipedia.org/wiki/Utility_system
- Practical utility AI (GDC 2024): https://schedule.gdconf.com/session/ai-summit-practical-utility-ai/902394
- Goal-oriented action planning, three states and a plan (GDC 2006): https://pages.cs.wisc.edu/~dyer/cs540/handouts/gdc2006_orkin_jeff_fear.pdf
- Emergent economies for role-playing games (2010): https://ianparberry.com/pubs/econ.pdf
- An open-source take on that economy: https://gamedev.net/tutorials/programming/general-and-gameplay-programming/bazaarbot-an-open-source-economics-engine-r3206
- Floating point determinism: https://gafferongames.com/post/floating_point_determinism
- Cross-platform determinism in practice (GDC 2024): https://media.gdcvault.com/gdc2024/Slides/GDC+slide+presentations/Pollard_Bradley_CrossPlatformDeterminism+2024-03-26+09.34.19.pdf
