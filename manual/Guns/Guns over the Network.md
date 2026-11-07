---
order: 5
---

Guns need nothing special for networked play (see [[Networking]]), but who decides what matters:

- **Hits are the server's.** Fire on the server, in a `[Predicted]` behaviour's FixedUpdate so the player's own machine fires at once too, and check hits with `Net.RaycastAsSeenBy`, which looks at the world as the player saw it when they pressed the trigger. See [[Lag Compensation]].
- **Ammo is the server's.** Keep the count in a `[Replicated]` property and set `gun.Rounds` from it, so a player can't give themself more.
- **Everyone sees everyone's hands.** Each machine holds every character's gun before their own eyes, from what it knows of where they look. Only this machine's own player's view moves the camera, hides their head and takes the mouse.
- **Actions play everywhere.** Ask for a reload on every machine, from a `[ClientRpc]` the server sends, and the same beats play on each.
