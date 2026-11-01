---
order: 1
---

The Console shows every message written while you work: by your scripts, by the editor, by the engine, and by the C# compiler when your code has a mistake in it. It sits under the Scene view.

Each row shows when the message was written, the message itself, and where it came from. Errors and warnings stand out in colours of their own.

## Writing to the Console

```csharp
Log.Info("The door opened");
Log.Warning("The door has no key set");
Log.Error("The door could not find its frame");
```

## Saying what a message is about

Give a log call the Obj, behaviour or prefab the message is about, after the message:

```csharp
Log.Warning("This door has no key set", Obj);
Log.Info("Spawning an enemy", enemyPrefab);
```

Clicking the message in the Console then shows that thing: an Obj lights up in the Hierarchy, and a prefab in the Project panel, opened to its folder. What you have selected stays as it was.

## Finding the code behind a message

Click a message to see it in full below the list, with its **stack trace**: the chain of methods that were running when it was written, the last one first. Each part is coloured: the namespace, the class, the method, its parameters, and the file and line.

- Click a file and line in the trace to open your code editor there.
- Double-click a message to open the line that wrote it.
- If the message says what it is about, clicking the message lights that Obj or asset up for a moment.

**My code only** folds away the parts of the trace that are not in your project, such as the engine's own code, so your own methods stand out. **Namespaces** and **Parameters** show or hide those parts of each line. **Hide libraries…** folds away a library you do not want to see in any trace. **Copy** puts the message and its trace on the clipboard.

Your code editor is set in **Settings**: VS Code, Rider, Zed, or whatever your system opens code with.

## Sorting through many messages

- **The search box** finds messages with every word you type. `tag:Combat` finds messages in a category, `-tag:Combat` leaves them out, and a pattern between slashes, as `/door \d+/`, is a regular expression.
- **The three counts**, for messages, warnings and errors, each show or hide that kind.
- **Collapse** shows each repeated message once, with how many times it came.
- **Clear** empties the Console.
- **Error Pause** pauses the game as soon as it logs an error, so you can look around at that moment.

## Message categories

Every message belongs to a category, worked out for you:

- A message that starts with a word in square brackets, as `Log.Info("[Combat] Hit for 12")`, is in that category: Combat.
- Otherwise, a script's message is in its namespace, as MyGame.
- The engine's messages are in Engine, Render, UI and so on, and the compiler's in Compiler.

**Categories** lists them all with how many messages each has. Untick one to hide it. Right-click a message to give its category a colour, which shows on every row in it. The colours are saved in `Settings/Console.toml`, so your whole team sees the same ones.

**Filters** saves the search, and the kinds and categories you have hidden, under a name, so you can come back to them in one click.

## Muting a message

Right-click a message and choose **Mute this message** to stop it showing. **Muted**, in the toolbar, lists what you have muted and brings it back.

## Messages from a build

When you start a build with **Run** in the Build panel, its messages come into the Console too, in the Player category. A build run on its own keeps its messages in a file; **Player Log** reads the last run of each build back into the Console.

## The Console's settings

The gear at the end of the toolbar opens the Console's own settings, with a sample that shows each change as you make it: row height, colouring rows by kind, alternating rows, how categories show, which parts of a trace show, the colours of each part, how many messages are kept, and whether to clear the Console each time you press Play.
