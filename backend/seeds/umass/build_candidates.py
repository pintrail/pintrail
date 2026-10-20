#!/usr/bin/env python3
"""Turn a raw snapshot into one list of candidate artifacts, matched and deduped.

    python3 seeds/umass/build_candidates.py                  # newest raw/<date>/
    python3 seeds/umass/build_candidates.py raw/2026-10-20

Writes candidates/candidates.json (every candidate, one common shape) and
candidates/report.md (a readable list to curate artifacts.toml from). Both
are gitignored: they carry UMass's own description text, which is for
reference while writing our own descriptions, never for publishing.

What it does:
  - normalizes the Sustainability Explorer, Energy Benchmarking, OSM and
    Wikidata sources into one candidate shape
  - sets each candidate's building from the benchmarking footprints
    (point in polygon), falling back to the nearest named OSM building
  - groups candidates from different sources that are the same thing
    (similar name, within 60 m)
  - flags Explorer records that are programs rather than places, and the
    ones dated before 2015, for a keep-or-drop decision

Standard library only.
"""

from __future__ import annotations

import argparse
import json
import math
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent

# Explorer categories and meta-categories that describe programs, courses or
# groups rather than a place someone can walk up to.
PROGRAM_CATEGORIES = {"Advocacy"}
PROGRAM_METACATS = {"Research and Initiatives"}


def clean(value) -> str | None:
    if value is None:
        return None
    text = re.sub(r"\s+", " ", str(value)).strip()
    return text or None


def norm_name(name: str) -> str:
    """Lowercase words with punctuation, articles and filler removed, for matching."""
    name = name.lower().replace("&", " and ")
    name = re.sub(r"[^a-z0-9 ]+", " ", name)
    stop = {"the", "of", "at", "and", "umass", "amherst", "building", "center", "hall", "house"}
    return " ".join(w for w in name.split() if w not in stop)


def similar(a: str, b: str) -> bool:
    wa, wb = set(norm_name(a).split()), set(norm_name(b).split())
    if not wa or not wb:
        return False
    return len(wa & wb) / min(len(wa), len(wb)) >= 0.6


def metres(lat1, lng1, lat2, lng2) -> float:
    r = 6371000.0
    p1, p2 = math.radians(lat1), math.radians(lat2)
    dp, dl = p2 - p1, math.radians(lng2 - lng1)
    a = math.sin(dp / 2) ** 2 + math.cos(p1) * math.cos(p2) * math.sin(dl / 2) ** 2
    return 2 * r * math.asin(math.sqrt(a))


def in_ring(lng: float, lat: float, ring: list) -> bool:
    inside = False
    j = len(ring) - 1
    for i in range(len(ring)):
        xi, yi = ring[i][0], ring[i][1]
        xj, yj = ring[j][0], ring[j][1]
        if (yi > lat) != (yj > lat) and lng < (xj - xi) * (lat - yi) / (yj - yi) + xi:
            inside = not inside
        j = i
    return inside


def in_polygon(lng: float, lat: float, geometry: dict) -> bool:
    polys = geometry["coordinates"] if geometry["type"] == "MultiPolygon" else [geometry["coordinates"]]
    for poly in polys:
        if in_ring(lng, lat, poly[0]) and not any(in_ring(lng, lat, hole) for hole in poly[1:]):
            return True
    return False


def centroid(geometry: dict) -> tuple[float, float]:
    ring = (geometry["coordinates"][0][0] if geometry["type"] == "MultiPolygon" else geometry["coordinates"][0])
    xs = [p[0] for p in ring]
    ys = [p[1] for p in ring]
    return sum(ys) / len(ys), sum(xs) / len(xs)


def load_buildings(snapshot: Path) -> list[dict]:
    """One record per benchmarked building, from its most recent fiscal year."""
    feats = json.loads((snapshot / "energy_benchmarking.geojson").read_text())["features"]
    latest: dict[str, dict] = {}
    for f in feats:
        p = f["properties"]
        if not f.get("geometry") or p.get("BLD_ID") is None:
            continue
        key = str(p["BLD_ID"])
        if key not in latest or str(p.get("FY_Year")) > str(latest[key]["properties"].get("FY_Year")):
            latest[key] = f
    out = []
    for key, f in sorted(latest.items()):
        p = f["properties"]
        lat, lng = centroid(f["geometry"])
        out.append({
            "id": f"bench:{key}",
            "source": "energy_benchmarking",
            "name": clean(p.get("Building_Name")),
            "lat": round(lat, 7),
            "lng": round(lng, 7),
            "building_id": key,
            "facts": {
                "fiscal_year": p.get("FY_Year"),
                "year_built": p.get("Year_Built"),
                "building_type": clean(p.get("Building_Type")),
                "gross_sq_ft": p.get("GSF"),
                "site_eui_kbtu_per_sf": p.get("Site_EUI_KBtu_per_sf"),
                "ghg_metric_tons": p.get("Total_GHG_Emission_Metric_Tons"),
            },
            "geometry": f["geometry"],
            "license": "UMass, none stated",
        })
    return out


def load_explorer(snapshot: Path) -> list[dict]:
    feats = json.loads((snapshot / "sustainability_explorer.geojson").read_text())["features"]
    out = []
    for f in feats:
        p = f["properties"]
        lng, lat = f["geometry"]["coordinates"][:2]
        date = clean(p.get("ProjectDat"))
        year = re.search(r"(19|20)\d\d", date or "")
        flags = []
        if p.get("Category") in PROGRAM_CATEGORIES or p.get("MetaCat") in PROGRAM_METACATS:
            flags.append("program, not a place")
        # Programs and services change faster than buildings; an old start
        # date on one is worth checking before it becomes an artifact.
        if year and int(year.group()) < 2015 and p.get("Category") not in {"Building", "Energy", "Water", "Landscape"}:
            flags.append(f"dated {year.group()}: check it still runs")
        out.append({
            # OBJECTID and GlobalID are missing on most records; GlobalID_2 is
            # set on every one and survives edits to the layer.
            "id": f"explorer:{clean(p.get('GlobalID_2')).strip('{}').lower()}",
            "source": "sustainability_explorer",
            "name": clean(p.get("Name")),
            "category": p.get("Category"),
            "metacat": p.get("MetaCat"),
            "lat": round(lat, 7),
            "lng": round(lng, 7),
            "location_text": clean(p.get("Location")),
            "project_date": date,
            "explorer_building_id": clean(p.get("Building_I")),
            "source_url": clean(p.get("More_Info")),
            "leed": clean(p.get("LEED_Certi")),
            "source_text": clean(p.get("Desc_")),
            "flags": flags,
            "license": "UMass, none stated: paraphrase, do not copy",
        })
    return out


def osm_kind(tags: dict) -> str:
    if tags.get("generator:source") == "solar":
        return "solar"
    if tags.get("amenity") == "charging_station":
        return "ev charging"
    if tags.get("tourism") == "artwork":
        return "artwork"
    if "building" in tags:
        return "building"
    return "other"


def load_osm(snapshot: Path) -> list[dict]:
    elements = json.loads((snapshot / "osm.json").read_text())["elements"]
    out = []
    for e in elements:
        tags = e.get("tags", {})
        lat = e.get("lat", e.get("center", {}).get("lat"))
        lng = e.get("lon", e.get("center", {}).get("lon"))
        if lat is None or lng is None:
            continue
        kind = osm_kind(tags)
        name = clean(tags.get("name"))
        if kind in {"building", "other"} and not name:
            continue
        out.append({
            "id": f"osm:{e['type']}/{e['id']}",
            "source": "osm",
            "name": name or kind,
            "osm_kind": kind,
            "lat": round(lat, 7),
            "lng": round(lng, 7),
            "wikidata": tags.get("wikidata"),
            "source_url": f"https://www.openstreetmap.org/{e['type']}/{e['id']}",
            "tags": {k: v for k, v in tags.items() if k in {
                "name", "building", "amenity", "tourism", "artwork_type", "artist_name", "start_date",
                "generator:source", "generator:output:electricity", "capacity", "operator", "wikidata"}},
            "license": "ODbL: (c) OpenStreetMap contributors",
        })
    return out


def load_wikidata(snapshot: Path) -> list[dict]:
    rows = json.loads((snapshot / "wikidata.json").read_text())["results"]["bindings"]
    out = []
    for r in rows:
        m = re.match(r"Point\(([-\d.]+) ([-\d.]+)\)", r["coord"]["value"])
        if not m:
            continue
        qid = r["item"]["value"].rsplit("/", 1)[-1]
        out.append({
            "id": f"wikidata:{qid}",
            "source": "wikidata",
            "name": r["itemLabel"]["value"],
            "lat": round(float(m.group(2)), 7),
            "lng": round(float(m.group(1)), 7),
            "wikidata": qid,
            "source_url": r.get("article", {}).get("value") or r["item"]["value"],
            "description": r.get("itemDescription", {}).get("value"),
            "image": r.get("image", {}).get("value"),
            "license": "CC0; image licensed per file on Commons",
        })
    return out


def assign_buildings(cands: list[dict], buildings: list[dict], osm: list[dict]) -> None:
    named_osm = [c for c in osm if c["osm_kind"] == "building"]
    for c in cands:
        if c["source"] == "energy_benchmarking":
            continue
        for b in buildings:
            if in_polygon(c["lng"], c["lat"], b["geometry"]):
                c["building"] = {"id": b["id"], "name": b["name"], "how": "inside footprint"}
                break
        else:
            best = min(named_osm, key=lambda o: metres(c["lat"], c["lng"], o["lat"], o["lng"]), default=None)
            if best and best["id"] != c["id"]:
                d = metres(c["lat"], c["lng"], best["lat"], best["lng"])
                if d <= 40:
                    c["building"] = {"id": best["id"], "name": best["name"], "how": f"nearest OSM building, {d:.0f} m"}


def group_duplicates(cands: list[dict]) -> None:
    """Give candidates that are the same real thing a shared group id."""
    next_group = 0
    for i, a in enumerate(cands):
        for b in cands[i + 1:]:
            if a["source"] == b["source"] or not a.get("name") or not b.get("name"):
                continue
            same_wd = a.get("wikidata") and a.get("wikidata") == b.get("wikidata")
            close = metres(a["lat"], a["lng"], b["lat"], b["lng"]) <= 60
            if same_wd or (close and similar(a["name"], b["name"])):
                g = a.get("group") or b.get("group")
                if g is None:
                    g = f"g{next_group}"
                    next_group += 1
                a["group"] = b["group"] = g


def report(cands: list[dict]) -> str:
    lines = ["# Candidates", "", "Generated by build_candidates.py. Private: contains UMass text. Do not commit.", ""]
    explorer = [c for c in cands if c["source"] == "sustainability_explorer"]
    for cat in sorted({c["category"] or "-" for c in explorer}):
        lines += [f"## Explorer: {cat}", ""]
        for c in sorted((c for c in explorer if (c["category"] or "-") == cat), key=lambda c: c["name"] or ""):
            bld = c.get("building", {}).get("name", "-")
            flags = f" **[{'; '.join(c['flags'])}]**" if c["flags"] else ""
            lines.append(f"- `{c['id']}` **{c['name']}** ({c['lat']}, {c['lng']}); building: {bld}; "
                         f"{c['project_date'] or 'no date'}; {c['source_url'] or 'no link'}{flags}")
            if c["source_text"]:
                lines.append(f"  > {c['source_text']}")
        lines.append("")
    for kind in ("solar", "ev charging", "artwork"):
        rows = [c for c in cands if c["source"] == "osm" and c["osm_kind"] == kind]
        lines += [f"## OSM: {kind} ({len(rows)})", ""]
        for c in rows:
            bld = c.get("building", {}).get("name", "-")
            lines.append(f"- `{c['id']}` {c['name']} ({c['lat']}, {c['lng']}); building: {bld}; {c['tags']}")
        lines.append("")
    lines += ["## Wikidata", ""]
    for c in (c for c in cands if c["source"] == "wikidata"):
        lines.append(f"- `{c['id']}` {c['name']}: {c.get('description') or ''} ({c['lat']}, {c['lng']}) image: {c.get('image') or '-'}")
    lines += ["", "## Benchmarked buildings", ""]
    for c in (c for c in cands if c["source"] == "energy_benchmarking"):
        f = c["facts"]
        lines.append(f"- `{c['id']}` {c['name']} ({c['lat']}, {c['lng']}): built {f['year_built']}, "
                     f"{f['building_type']}, {f['gross_sq_ft']} sq ft, EUI {f['site_eui_kbtu_per_sf']}, {f['fiscal_year']}")
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("snapshot", nargs="?", type=Path, help="a raw/<date> directory (default: newest)")
    args = parser.parse_args()
    snapshot = args.snapshot or max((HERE / "raw").glob("*/"), default=None)
    if snapshot is None or not snapshot.is_dir():
        print("no snapshot found; run fetch.py first", file=sys.stderr)
        return 1

    buildings = load_buildings(snapshot)
    explorer = load_explorer(snapshot)
    osm = load_osm(snapshot)
    wikidata = load_wikidata(snapshot)
    cands = buildings + explorer + osm + wikidata
    assign_buildings(cands, buildings, osm)
    group_duplicates(cands)

    out = HERE / "candidates"
    out.mkdir(exist_ok=True)
    for c in buildings:
        c.pop("geometry", None)
    (out / "candidates.json").write_text(json.dumps({"snapshot": snapshot.name, "candidates": cands}, indent=1) + "\n")
    (out / "report.md").write_text(report(cands))

    placed = sum(1 for c in explorer if c.get("building"))
    flagged = sum(1 for c in explorer if c["flags"])
    groups = len({c["group"] for c in cands if c.get("group")})
    print(f"{len(cands)} candidates from {snapshot.name}: {len(buildings)} buildings, {len(explorer)} explorer, "
          f"{len(osm)} osm, {len(wikidata)} wikidata", file=sys.stderr)
    print(f"explorer: {placed} placed in a building, {flagged} flagged; {groups} cross-source duplicate groups", file=sys.stderr)
    print(f"wrote {out / 'candidates.json'} and {out / 'report.md'}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
