---
order: 4
---

Pages and code can point at each other, so whoever reads one can find the other.

## The Code Reference

The **Code Reference**, at the bottom of the tree, has a page for every public type in your scripts and in Voxear. Each page shows how the type is declared and what its `///` comments say, then its fields, properties, methods and events, each with its own comment.

```csharp
/// <summary>Turns the Obj it is on around its up axis.</summary>
public sealed class Spinner : Behaviour
{
	/// <summary>How fast it turns, in degrees a second.</summary>
	public float Speed = 90;
}
```

These pages are made again each time your scripts build, so to change what one says, change the comments in the code.

You can still add your own words to a type's page: click **Edit** on it and write. What you write is the page's **note**. It shows under the code's own words, and it is kept in `Docs/.code-notes`, so it lasts when the page is made again.

Any page can link to a type:

```markdown
[[code:MyGame.Spinner]]
[[code:MyGame.Spinner|the spinner]]
```

## Linking code to a page

Put `[Documentation]` on a behaviour, a component, or one of their fields, and give it the page's path or title:

```csharp
[Documentation("Combat/Damage")]
public sealed class Health : Behaviour
{
	[Documentation("Combat/Damage#armour")]
	public float Armour;
}
```

The Inspector then shows a book button on the behaviour and beside the field. Clicking it opens the page, at the heading named after the `#` if there is one.

To get the attribute without typing it, right-click a page in the tree and choose **Copy [Documentation] attribute**.

## Showing part of a page in the Inspector

`[Excerpt]` shows a section of a page right above a field in the Inspector, folded shut until you open it:

```csharp
[Excerpt("Combat/Damage#falloff")]
public float Falloff = 0.5f;
```

The section runs from that heading to the next heading of the same size or bigger. It updates as soon as the page changes, so what you write in VoxDocs shows in the Inspector straight away.

## Seeing both sides

A page lists the code that points at it under **In the code**, each linking to the type's Code Reference page. A type's Code Reference page says which page documents it.

If a `[Documentation]` or `[Excerpt]` names a page that is not there, **Health** lists it. See [[Docs Health]].
