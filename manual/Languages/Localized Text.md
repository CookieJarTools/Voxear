---
order: 3
---

A **Localized Text** shows a key's text in an element of its Obj's UI document, in the player's language, with no script: a sign over a tavern door that reads **The Green Dragon** in English and **Le Dragon Vert** in French, or a line of a HUD. It follows the language, so when the player picks another it changes at once.

It works beside a **UI Panel**, which shows a document on a rectangle in the world, or a **Screen UI**, which shows one over the game's view.

## A sign in the world

1. Make the sign's document, `UI/Sign.vui`, with a label named `title`:

   ```xml
   <vui>
     <Label name="title" />
   </vui>
   ```

2. Add the key to a string table ([[Translating Your Game]]):

   ```toml
   [[entry]]
   key = "sign.tavern"
   en = "The Green Dragon"
   fr = "Le Dragon Vert"
   ```

3. Choose **UI Panel** from the Hierarchy's **+** button, and pick `UI/Sign.vui` as its **Document**.
4. Click **Add Component** and choose **Localized Text**, in the UI folder.
5. Pick its **Key** from the list of your tables' keys, beside their text, and its **Element**: `title`. **First label** shows it in the document's first label.

Play, and pick French from a script or the console: the sign reads **Le Dragon Vert**.

## Values

A key's text can name values, as `{owner}'s {count, plural, one {room} other {rooms}}`. The Inspector shows a field for each value the key's text names, under **Key** and **Element**. Type what each one is; a number picks plurals and is written as the language writes numbers.

## In a scene file

```toml
[obj.localized_text]
key = "sign.tavern"
element = "title"
values = { owner = "Mara", count = "3" }
```

## From a script

```csharp
var sign = Obj.LocalizedText;
sign.Key = Strings.Sign.Tavern;
sign.SetValue("owner", innkeeper.Name);
sign.SetValue("count", 3);
Log.Info(sign.Text);
```

| Member | What it does |
| --- | --- |
| `Key` | The key it shows, as a `LocalizedString` |
| `Element` | The name of the element it shows in; empty for the first label |
| `SetValue(name, value)`, `GetValue(name)` | Fill in, or read, a value its key's text names |
| `RemoveValue(name)`, `ClearValues()` | Take one or every value out |
| `Text` | Its text as it shows now, in the game's language |

`Obj.AddLocalizedText()` gives an Obj one. It sets its element's text each frame the two differ, so a script that changes that element's text directly is overruled: change its key or its values instead.
