Artifacts can sit **inside** other artifacts. A building holds its roof, its rooms, and its systems; a room can hold a display or a sign. The artifact something sits in is its **parent**.

## An example

```
Integrative Learning Center          (building)
├── Green roof                       (rooftop)
├── Stormwater reuse for irrigation  (installation)
└── LEED feature signs               (installation)

Central Heating Plant                (building)
├── Combined heat and power system   (installation)
└── Battery energy storage system    (installation)
```

In the Studio's list on the left, nested artifacts appear indented under their parent. On an artifact's page, **Nested inside** lists what's inside it.

## Why nest?

- **The app shows them together.** Someone at the building sees what's on and in it.
- **They share a location.** A nested artifact can use its parent's pin, so you don't have to place it, and if the building's pin is ever corrected, everything inside moves with it. See [Location](/studio/help/location#using-the-parent-s-location).
- **It reads naturally.** *Green roof*, inside the *Integrative Learning Center*, needs no longer name.

## When to nest, and when not to

Nest something when it is physically **part of, in, or on** the other thing.

| Situation | Nest it? |
|---|---|
| A green roof on a building | Yes, inside the building |
| A battery system inside the heating plant | Yes, inside the plant |
| A rain garden next to a building, but not part of it | No, give it its own location. Or nest it and give it its own pin, if it belongs to the building's story. |
| Six bike share stations around campus | No. They're separate places that share one story: use a [topic](/studio/help/topics). |
| Every LEED-certified building | No. Link each one to the *LEED certification* topic. |

> **Tip:** Nesting is about **where** something is. A [topic](/studio/help/topics) is about **what** several things have in common. See [Kinds, tags, topics, or nesting?](/studio/help/tags-kinds-topics)

## How deep can it go?

As deep as makes sense: campus, building, floor, room, display. Most artifacts are one level down from a building. Don't add a level just for tidiness; *Third floor* is only worth an artifact if there's something to say about it.

## Adding and moving

- To add something inside an artifact, open the artifact and click **+ Add one inside**. See [Adding an artifact inside another](/studio/help/adding-inside).
- To move an artifact to a different parent, **Edit** it and change **Inside (parent)**. Everything nested in it moves too.
- An artifact can't be put inside itself, or inside something that is already inside it.
