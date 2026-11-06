# UMass sustainability seed

About 60 real campus sustainability artifacts, built from public UMass data, so search, Q&A and the explorer have real content to work with (#81). The artifacts are UMass buildings, solar arrays, gardens, stormwater features, EV chargers and more.

| File | What it is | Committed? |
|---|---|---|
| `artifacts.toml` | **The curated artifacts.** Our own descriptions, tags, parents, coordinates and source links. Edit this. | yes |
| `seed-dev.sql` | Generated. Loads everything **approved**, for development. | yes |
| `seed-prod.sql` | Generated. Loads everything as **drafts**, for review in the Studio. | yes |
| `remove.sql` | Generated. Deletes everything the seed loaded. | yes |
| `fetch.py` | Downloads the public sources into `raw/<date>/`. | yes |
| `build_candidates.py` | Normalizes, matches and dedupes a snapshot into `candidates/`. | yes |
| `generate_sql.py` | Turns `artifacts.toml` into the three SQL files. | yes |
| `raw/`, `candidates/` | Downloaded data and the candidate list. | **no** (gitignored) |

All the scripts use only the Python standard library and need Python 3.11 or newer.

## Loading it

From `backend/`, with the database running:

```sh
# Local development: everything approved, so the app and search see it at once
docker compose exec -T postgres psql -U pintrail -d pintrail -v ON_ERROR_STOP=1 < seeds/umass/seed-dev.sql

# Production (on the server): everything arrives as a draft for review
docker compose exec -T postgres psql -U pintrail -d pintrail -v ON_ERROR_STOP=1 < seeds/umass/seed-prod.sql

# Take it all out again
docker compose exec -T postgres psql -U pintrail -d pintrail -v ON_ERROR_STOP=1 < seeds/umass/remove.sql
```

- **Safe to run more than once.** Ids are fixed and start with `5eed0081-`, so a second run adds nothing.
- **Production never overwrites.** An artifact that is already loaded, perhaps edited since in the Studio, is left alone.
- **Development can overwrite.** To make an existing dev database match the file, generate with `--update` and load that (see below).
- **Search picks it up automatically.** Loading queues every artifact for search indexing (#64), so the worker fills `search_documents` over the next few minutes.
- **Removal is cautious.** `remove.sql` refuses to run while any artifact an author created sits under a seed artifact, because deleting a parent deletes its children too. Their history rows stay.
- **History.** As with `seeds/examples.sql`, changes made this way show in each artifact's history as made by "system".

## Changing the artifacts

1. Edit `artifacts.toml`. The comment at its top explains every field. Keep `key`s stable: the database id is derived from the key, so changing one makes a new artifact.
2. Regenerate the SQL and commit it with the change:
   ```sh
   python3 seeds/umass/generate_sql.py --check                    # validate, and count the open "check" notes
   python3 seeds/umass/generate_sql.py --mode dev  > seeds/umass/seed-dev.sql
   python3 seeds/umass/generate_sql.py --mode prod > seeds/umass/seed-prod.sql
   python3 seeds/umass/generate_sql.py --remove    > seeds/umass/remove.sql
   ```
   CI fails if `artifacts.toml` is invalid or the committed SQL doesn't match it.
3. To update a dev database that already has the seed: `python3 seeds/umass/generate_sql.py --mode dev --update | docker compose exec -T postgres psql ...`. This replaces the seed's text, tags, links and topic links with the file's. It's dev only, because in production it would wipe out reviewers' edits.

## Fact checking

Every description was drafted from the sources and should be checked by a person before the artifact is approved in production.

- About 25 artifacts carry a `check = "..."` line naming the specific thing to confirm. Typical examples: a garden that may no longer be tended, a ValleyBike station from 2018, or a solar canopy's lot number. When a check is done, fix the description if needed and delete the line.
- The rest still need a read-through against their source links.

## Where the data comes from

`fetch.py` saves each source untouched, with a `sources.json` manifest:

| Source | What it gives | License |
|---|---|---|
| [UMass Sustainability Explorer (2018)](https://services.arcgis.com/2gdL2gxYNFY2TOUb/arcgis/rest/services/UMass_Sustainability_Explorer_2018/FeatureServer/0) | 144 sustainability sites with coordinates, category, description, link, LEED | None stated; UMass-owned |
| [UMass Energy Benchmarking Explorer V3](https://services.arcgis.com/2gdL2gxYNFY2TOUb/arcgis/rest/services/UMass_Energy_Benchmarking_Explorer_V3/FeatureServer/0) | Footprints and facts for 104 buildings (to FY15) | None stated; UMass-owned |
| OpenStreetMap (Overpass API) | Named buildings, solar arrays and canopies, EV chargers, public art | ODbL: © OpenStreetMap contributors |
| Wikidata | Items at UMass Amherst with coordinates | CC0 |

Descriptions were then written from those records and the current pages they link to:

- [Planning, Design & Construction building pages](https://www.umass.edu/planning-design-construction/book/buildings-infrastructure)
- [UMass Sustainability: operations and infrastructure](https://www.umass.edu/sustainability/operations-infrastructure)

To refresh the sources:

```sh
python3 seeds/umass/fetch.py              # writes raw/<today>/
python3 seeds/umass/build_candidates.py   # writes candidates/candidates.json and candidates/report.md
```

`candidates/report.md` is the list to curate from. It groups the Explorer records by category and shows each one's matched building (point-in-polygon against the benchmarking footprints, or the nearest named OSM building within 40 m) and its source text. Records that are programs rather than places, and old programs that may have ended, are flagged.

## What this repository may and may not contain

This repository is public, so:

- **Committed:** the scripts, our own descriptions in `artifacts.toml`, and links to the sources.
- **Not committed:** the raw UMass layers, page text and the candidate files, which quote UMass's own descriptions. The UMass layers state no license, and being publicly queryable is not a license to republish. `raw/` and `candidates/` are gitignored.
- **Credit:** where OpenStreetMap data is used (some locations, and links to its maps), credit "© OpenStreetMap contributors" wherever the data is shown.
- **Not copied:** UMass's own text and photos, including the Explorer's `ImageLink` photos. Reusing those on the public site needs permission from Campus Planning or the Sustainability office first.

## Known gaps

- Most facts are from the 2018 Explorer plus current PD&C and Sustainability pages. Programs, gardens and stations from 2018 may have changed; their `check` notes say so.
- Several old source links (all the `goo.gl` short links, and the old `/sustainability/...` and `localumass.com` pages) are dead. They were replaced by current pages, or by a map link where no current page was found.
- No photos yet. Photos will come only from Wikimedia Commons, with author and license stored.
