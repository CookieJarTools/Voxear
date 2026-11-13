---
order: 1
---

An **Item** is something a player carries. Right-click in the Project panel, choose **New Data Asset…** and pick **Items ▸ Item**. An item is called what its file is.

| Setting | What it does |
| --- | --- |
| Icon | Its picture in an inventory |
| Stack | How many of it go in one place of an inventory |
| Places | The [[Blocks|block]] it puts down |
| Leaves | The item left in the hand once it is put down, as a bucket is by a bucket of water |
| Dropped | What it is when it lies in the world: a prefab, in place of a small block of what it places |
| Tool | The [[Tool Kinds and Stations|kind of tool]] it is |
| Tier | How good a tool it is: a block drops what it does only to a tool of its tier or higher |
| Speed | How many times faster than a hand it breaks the blocks that ask for its kind |
| Durability | How many blocks it breaks before it is worn out; 0 never wears |

## The item a block is carried as

A block and the item that places it are two assets: the block is what stands in the world, the item is what lies in an inventory. A block's Inspector has **Make Item**, which makes the item, sets its **Places**, and has the block drop it.

They are separate so each can be more than the other: stone drops cobblestone rather than itself, a seed places a crop it is not, and a pickaxe places nothing at all.

## Buckets

An item whose **Places** is a liquid and whose **Leaves** is another item holds that liquid in the other: a Water Bucket places water and leaves a Bucket. That is all a bucket is. With the empty one in hand, the liquid can be taken up again (see [[Blocks and Items from Scripts#Liquids and buckets|Liquids and buckets]]).

## Items on the ground

What a felled tree breaks into lies on the ground as items, drawn as small blocks of what they place, or as the item's **Dropped** prefab where it has one. An **Item Pickup** on a player takes them up as it walks over them; see [[Blocks and Items from Scripts#Items on the ground|Items on the ground]].

## Items of your own

Derive from `Item` to add fields, and your data assets of it are items everywhere an item goes:

```csharp
[CreateAsset("Items/Food")]
public sealed class Food : Item
{
    public float Hunger = 4f;
}
```

`SpeedOn(block)` and `Harvests(block)` are virtual, for a tool with rules of its own.

## Items in a build

An item ships when something names it: a block that drops it, a recipe, a field. `Items.All` finds the ones that shipped.
