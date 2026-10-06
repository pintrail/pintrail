# Adding artifacts in the Pintrail Studio

A step-by-step guide for authors adding campus artifacts to Pintrail.

An **artifact** is anything on campus worth stopping for: a building, a room, a rooftop, a piece of art, or an installation such as a solar canopy or a rain garden. Artifacts can sit **inside** other artifacts. A building can hold its green roof, its stormwater system, or a room, and those can hold their own artifacts in turn. Later, artifacts get grouped into **trails** that people walk with the Pintrail app.

You do all of this in the **Studio**, a website you use in your browser:

**https://pintrail.cs.umass.edu/studio**

## Before you start

You need a Studio account. Your instructor creates it and gives you two things:

- the email address the account uses, and
- a **temporary password**.

## 1. Sign in and choose your own password

Go to the Studio and sign in with your email and the temporary password.

![The Studio sign-in page](img/add-artifact/01-sign-in.png)

The first time you sign in, the Studio asks you to choose your own password. Enter the temporary password as your current password, then a new one of **at least 12 characters**, twice. You can't do anything else until you've done this.

![Choosing a new password on first sign-in](img/add-artifact/02-choose-password.png)

You can change it again at any time with **Change password** at the top right.

## 2. Find your way around

The Studio has two parts:

- **The sidebar on the left** lists every artifact. Artifacts that sit inside another one are indented beneath it. Click any artifact to open it.
- **The main area** shows the artifact you opened, or the form when you create or edit one.
- **Map**, at the top of the sidebar, shows every artifact on one map (see [step 8](#8-see-everything-on-the-map)).

![The Studio, with the example artifacts in the sidebar](img/add-artifact/03-studio-home.png)

## 3. Look at the examples first

Artifacts whose names start with **"Example:"** are worked examples. Open them before you write your own, and use them as a model:

- **Example: Integrative Learning Center** holds *Example: Green roof* and *Example: Stormwater reuse for irrigation*.
- **Example: Central Heating Plant** holds *Example: Combined heat and power system* and *Example: Battery energy storage system*.

At the bottom of a building's page, **Nested inside** lists the artifacts inside it.

![An example building, with its nested artifacts listed at the bottom](img/add-artifact/05-nested-list.png)

Please **don't edit or delete the examples**. Everyone uses them as a reference.

## 4. Add an artifact

Click **+ New** at the top of the sidebar. Then fill in the form. Each field has a short note under it explaining what goes there.

| Field | What to enter |
|---|---|
| **Name** | What people call it, for example *Old Chapel* or *Lot 49 solar canopy*. |
| **Kind** | One of `building`, `room`, `artwork`, `installation`, `rooftop`, or `other`. Use `installation` for equipment and site features (solar arrays, batteries, rain gardens, bike racks). |
| **Inside (parent)** | Leave as **none (top level)** for a building or anything standing on its own. To put it inside another artifact, choose that artifact (see step 5). |
| **Description** | The heart of the artifact. See [Writing a good description](#writing-a-good-description) below. |
| **Tags** | Themes someone might look for, such as *solar*, *stormwater*, or *energy storage*. Type a tag and press **Enter**; click **×** on a tag to remove it. As you type, existing tags are suggested: pick one when it fits, so the same theme isn't spelled two ways. |
| **Location** | **Required for a top-level artifact.** Click the map where the artifact is, and a pin appears. Zoom in first (the **+** button, or scroll) so you can place it precisely. The latitude and longitude boxes fill in for you. You can also type or paste coordinates into the boxes and the pin moves to match. A pair copied from Google Maps, like `42.3891, -72.5281`, can be pasted straight into the latitude box. |

![Filling in a new artifact, with the pin placed on the map](img/add-artifact/06-new-artifact-form.png)

**Check the pin before you save.** Zoom in and make sure it sits on the right building or spot, not on the one next door. The pin is what the app uses to tell someone they're nearby.

Click **Create**. The artifact opens, and it now appears in the sidebar.

If something is missing, the form stays open with a red note at the top saying what to fix, and everything you typed is kept. The most common one: a top-level artifact with no location. Place it on the map, or choose the artifact it sits inside.

![The note shown when a top-level artifact has no location](img/add-artifact/13-location-required.png)

![The saved artifact](img/add-artifact/07-saved-artifact.png)

To change anything later, open the artifact and click **Edit**.

## 5. Add an artifact inside another one

Many sustainability features belong to a building: its roof, its heating system, a sign in its lobby. Add those *inside* the building, so the app can show them together.

1. Open the building.
2. Scroll to the bottom and click **+ Add one inside** (or **+ Add a nested artifact**).
3. The form opens with **Inside (parent)** already set to that building.

![A new artifact being added inside the Integrative Learning Center](img/add-artifact/08-nested-form.png)

**Usually, leave the location as it is.** The map opens on the parent's location, marked with a hollow, dashed pin, and the coordinate boxes show the parent's numbers in grey. That's the location this artifact uses, which is right for anything in or on the building.

![The location of a nested artifact, showing its parent's location](img/add-artifact/10-inherited-location.png)

If the feature really is somewhere else, such as a rain garden across the plaza, **drag the pin** to the right spot, or click the map. The pin turns solid, and the artifact now has its own location. **Clear (use parent's)** puts it back.

Leaving it on the parent's location matters: if someone later corrects the building's pin, everything inside it moves with it. After you save, the page shows where the location came from: **inherited from** the parent.

![A nested artifact using its building's location](img/add-artifact/09-nested-saved.png)

If you put something in the wrong parent, click **Edit** and change **Inside (parent)**.

## 6. Add links to your sources

Every fact should come from somewhere a reader can check. On an artifact's page, the **Links** section takes web addresses:

1. Paste the address into the first box. You can leave off `https://`.
2. Optionally, say what the link is in the second box, for example *Source for the energy figures*.
3. Click **+ Add link**. Pintrail reads the page and shows a preview with its title, summary, and image, like a link pasted into Notion or Slack. This takes a few seconds.

![The Links section with two sources added](img/add-artifact/11-links.png)

- **To edit** a link's address or its note, click the pencil. Change the address and Pintrail reads the new page for a fresh preview.
- **To reorder**, drag a link by the grip (the six dots) on its left. You can also click the grip and use the up and down arrow keys. Put the most important source first.
- **To remove** a link, click **×**.

![Editing a link](img/add-artifact/12-edit-link.png)

Some sites don't offer a preview. The link still works; it just shows the address instead of a title.

## 7. Add photos and PDFs

On an artifact's page, the **Media** section has a **+ Upload** button. You can pick several photos and PDFs at once.

- New uploads show **processing…** for a moment while Pintrail makes thumbnails. The page updates by itself.
- If one shows **failed**, remove it with the **✕** and try again. If it keeps failing, tell your instructor.
- Use photos you took yourself, or that you have permission to use. A clear photo of the thing itself is worth more than several distant ones.

## 8. See everything on the map

Click **Map** at the top of the sidebar to see every artifact on one map. Each marker is an artifact with its own location; click it to see its name and tags, and the artifacts nested inside it. Click any name to open that artifact.

Use **Kind** and **Tag** above the map to show only, say, installations, or only artifacts tagged *solar*. It's a quick way to spot a pin in the wrong place, or a part of campus nobody has covered yet.

![The map of every artifact, with a building's popup open](img/add-artifact/14-map.png)

## Writing a good description

Someone will read this standing in front of the artifact, on their phone. Write for them. A good description answers:

- **What it is.** One or two sentences a visitor would understand.
- **Why it matters.** The sustainability story, with a real number where you have one: megawatts, gallons, percent saved, the year it opened.
- **How it works**, or what to notice when you're standing there.
- **Sources.** Add the pages your facts come from as [links](#6-add-links-to-your-sources), not inside the description.

Some tips:

- **Use paragraphs.** Press Enter twice between them. The Studio keeps your line breaks.
- **Write in your own words.** Don't paste text from a website; summarize it and cite it.
- **Only write what you can source.** If you aren't sure of a number, leave it out or ask Ezra Small (Campus Sustainability).
- **Keep it readable on a phone.** A few short paragraphs beats one long one.

The example artifacts follow this pattern, including their tags and source links. Compare yours with them.

## Deleting

**Delete** asks you to confirm, and **it also deletes everything nested inside the artifact**, along with its photos. If you want to keep a nested artifact, first edit it to move it to a different parent.

## Coming soon

- **Trails:** grouping artifacts into a walking route.

## Getting help

If something in the Studio doesn't work the way this guide says, email your instructor with a screenshot and the name of the artifact you were working on.
