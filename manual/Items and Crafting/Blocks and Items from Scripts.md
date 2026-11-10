---
order: 4
---

A block's rules and an item's are applied by a few calls on **Blocks**, so a script that digs and builds is short:

```csharp
private readonly BlockMiner miner = new();

if (Blocks.Raycast(eye, forward, out var hit, 6f))
{
    // Dig while the button is held: true on the frame the block breaks.
    if (Input.Held(dig) && miner.Dig(hit, held, out var drops)) inventory.Add(drops);
    // Put down what the hand holds.
    if (Input.Pressed(use) && held != null && Blocks.Place(hit, held)) inventory.SpendHeld();
}
else
{
    miner.Stop();
}
```

`held` is the `Item` in the player's hand, or null for a bare hand.

## What a block says

`hit.Definition` is the [[Blocks|block]] a ray met, and a `Block` field in the Inspector picks one.

| Member | What it gives |
| --- | --- |
| `Name`, `Color`, `IsLiquid` | What it is called, its colour, and whether it is a liquid |
| `Hardness`, `Unbreakable`, `Tool`, `Tier` | Its rules, as its Inspector has them |
| `Drops` | What it drops: each an item, the fewest and most of it, and its chance |
| `BreakSound`, `PlaceSound`, `StepSound` | Its sounds |
| `Data<T>()` | Its Data asset, as your own type |
| `BreakTime(held)` | How many seconds it takes to break with that item in hand |
| `DropsFor(held)` | Whether that item gets its drops |

A kind a world keeps itself, a colour alone, has no rules: its `Definition` is `Block.None`, and it breaks at once and drops nothing.

## Digging

| Call | What it does |
| --- | --- |
| `Blocks.Mine(hit, held)` | Breaks the block by its rules, at once: rolls its drops for the tool held, plays its sound, and gives the drops as `ItemStack`s |
| `BlockMiner.Dig(hit, held, out drops)` | Counts the time the block takes, and mines it once that has passed: call it every frame the dig button is held |
| `miner.Progress` | How far through the block it is, from 0 to 1, for a bar or cracks |
| `miner.Stop()` | Leaves the block: the next Dig starts from nothing |

An unbreakable block is never mined. `Blocks.Break(hit)` still takes it away, as it takes any block, whatever its rules: that one is for your own tools and the editor's.

## Placing

`Blocks.Place(hit, item)` puts down the block the item **Places** against the face the ray met, with its sound. It is false, and does nothing, for an item that places none or where there is no room. Taking one of the item from whoever holds it, and giving them what it **Leaves**, is the caller's to do, since only the caller knows what holds it.

## Liquids and buckets

A ray passes through liquids unless you ask it to meet them, and a block goes down through water, taking its place. To take a liquid up, cast the ray with liquids met and call `Blocks.Scoop`:

```csharp
if (Blocks.Raycast(eye, forward, out var wet, 6f, liquids: true) && Blocks.Scoop(wet, held) is { } full)
{
    inventory.TurnHeldInto(full);   // the bucket is a bucket of water now
}
```

`Scoop` gives the item that places that liquid and leaves the held one, and takes the liquid's block away. It gives null for an item that holds no liquid, and for water that is running rather than at rest.

## Recipes

```csharp
// What the items in a grid two wide make, by hand.
var recipe = Recipes.Match(grid, 2);
if (recipe != null) made = recipe.Makes;
```

`grid` is the grid's places row by row from the top left, each an `Item` or null, or each an `ItemStack`. Give a [[Tool Kinds and Stations#Stations|station]] as the third argument to match the recipes made there too. `Recipes.All` is every recipe, `Recipes.For(item)` those that make an item, and `Recipes.Add` adds one made in code.

## Hearing what happens

| Event | When |
| --- | --- |
| `Blocks.Mining` | A block is about to be mined: change `Drops`, or set `Cancel` to leave it where it is |
| `Blocks.Mined` | A block was mined |
| `Blocks.Placed` | An item put a block down |

They are raised by `Blocks.Mine` and `Blocks.Place(hit, item)`, not by the plain `Set`, `Fill` and `Break`.

## Rules of your own

For a block that breaks or drops by rules of its own, derive a data asset from `BlockRules` and give it to the block as its **Data**:

```csharp
[CreateAsset("Blocks/Ore Rules")]
public sealed class OreRules : BlockRules
{
    public Item? Gem;

    public override void Drops(Block block, Item? held, List<ItemStack> drops)
    {
        if (Gem != null && Random.Shared.NextSingle() < 0.05f) drops.Add(new ItemStack(Gem));
    }
}
```

`BreakTime`, `Drops`, `OnMined` and `OnPlaced` each start from the block's own answer, so override only what differs.

## Over the network

Decide on the server: call `Blocks.Mine` and `Blocks.Place` there, and tell the others which `Block` or `Item` went where. A block's number in a world is that machine's own where the world numbered the block in play, so it is never what to send.
