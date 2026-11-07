---
order: 16
---

Voxear plays games over the network with a server in charge: it runs the game, players send it what they press, and it decides what happens. One machine can host and play at once, or a dedicated server can run the game for everyone.

Networking is a module: switch it on in **Project Settings** first (see [[Modules]]).

1. [[Playing over the Network]]: hosting, joining, players and their input, and ticks
2. [[Sharing Objs]]: Net Objects, owners, priority and range, and replicated properties
3. [[RPCs]]: asking the server to do something, and telling players something happened
4. [[Predicting Movement]]: a player's own Objs moving at once, and put right when the server disagrees
5. [[Lag Compensation]]: checking a player's shots against the world as they saw it
6. [[Shared Authority]]: players' machines running their own Objs, for games played together

For a dedicated server's own build, see [[Server Builds]].
