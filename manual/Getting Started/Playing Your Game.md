---
order: 4
---

Click **Play**, the triangle at the top of the window, to run the level you have open. The Game view comes to the front and your scripts start. Click it again to stop.

The first time you play, the editor builds your C# scripts, so it takes a moment. The bar at the bottom says **Building scripts…** while it does. After that, starting is quick.

- **Pause**, beside Play, freezes the game. Click it again to carry on.
- **Step**, beside Pause, moves a paused game on by one frame (a sixtieth of a second), so you can watch something happen slowly.

## Changes made while playing

While the game plays you can still select things and change them in the Inspector, to try out values. **When you stop, the level goes back to how it was before you pressed Play.** Changes made while playing are not kept, and Ctrl+S and Undo wait until you stop.

To keep a value you found while playing, right-click it in the Inspector and choose **Keep value after play**. When you stop, it is copied into the level you are editing.

## Changing code while playing

Save a script while the game plays, and the editor builds it and swaps it into the running game, so you see the change without starting again. Each script keeps the values its fields had. To stop this, turn off **Reload changed scripts while playing** in Settings.

If your scripts do not build, the Console lists what is wrong, and the game keeps running the scripts as they last built.

## The Game view needs a camera

The Game view looks through the first camera in the level. A level with no camera shows "No camera renders this level." Add one with the Hierarchy's **+** button, then **Camera**. New projects start with one, called Main Camera.

## Play settings

Open **Settings**, from the strip on the far left, and look under **Play mode**:

| Setting | Does |
| --- | --- |
| Start paused | Starts the game paused, ready to Step |
| Save before play | Saves the level each time you press Play |
| Clear Console on play | Empties the Console each time you press Play |
| Pause on error | Pauses the game when it logs an error |
| Reload changed scripts while playing | Swaps saved scripts into the running game |

Next: [[Making a Build]].
