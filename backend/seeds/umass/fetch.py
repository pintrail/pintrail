#!/usr/bin/env python3
"""Download the public UMass data sources into a dated snapshot.

    python3 seeds/umass/fetch.py              # writes raw/<today>/
    python3 seeds/umass/fetch.py --out DIR    # somewhere else

Each source is saved untouched, exactly as the server returned it, plus a
sources.json manifest recording what was fetched, from where, and when.
Nothing here is committed: raw/ is gitignored, because the UMass layers
state no license and this repository is public (see README.md).

Standard library only, so it runs anywhere Python 3.11+ does.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import sys
import time
import urllib.parse
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
USER_AGENT = "pintrail-seed/1.0 (https://github.com/pintrail/pintrail; campus sustainability app)"

ARCGIS = "https://services.arcgis.com/2gdL2gxYNFY2TOUb/arcgis/rest/services"

# Campus bounding box: south, west, north, east. Covers the core campus, the
# Central Heating Plant, and the north and southwest residential areas.
BBOX = (42.375, -72.540, 42.405, -72.510)

OVERPASS_QUERY = """
[out:json][timeout:90];
(
  nwr["generator:source"="solar"]({s},{w},{n},{e});
  nwr["amenity"="charging_station"]({s},{w},{n},{e});
  nwr["tourism"="artwork"]({s},{w},{n},{e});
  nwr["building"]["name"]({s},{w},{n},{e});
  nwr["wikidata"]({s},{w},{n},{e});
);
out center tags;
""".format(s=BBOX[0], w=BBOX[1], n=BBOX[2], e=BBOX[3])

# Items located in, part of, or owned by UMass Amherst (Q15142), with
# coordinates, plus their image on Commons when Wikidata has one.
WIKIDATA_QUERY = """
SELECT DISTINCT ?item ?itemLabel ?itemDescription ?coord ?image ?article WHERE {
  { ?item wdt:P131 wd:Q15142 } UNION { ?item wdt:P361 wd:Q15142 }
  UNION { ?item wdt:P127 wd:Q15142 } UNION { ?item wdt:P276 wd:Q15142 }
  ?item wdt:P625 ?coord .
  OPTIONAL { ?item wdt:P18 ?image }
  OPTIONAL { ?article schema:about ?item ; schema:isPartOf <https://en.wikipedia.org/> }
  SERVICE wikibase:label { bd:serviceParam wikibase:language "en" }
}
"""


def request(url: str, data: bytes | None = None, accept: str = "application/json") -> bytes:
    req = urllib.request.Request(url, data=data, headers={"User-Agent": USER_AGENT, "Accept": accept})
    for attempt in range(3):
        try:
            with urllib.request.urlopen(req, timeout=120) as resp:
                return resp.read()
        except Exception as err:  # noqa: BLE001 - retry anything transient
            if attempt == 2:
                raise
            print(f"  retrying after {err}", file=sys.stderr)
            time.sleep(5 * (attempt + 1))
    raise AssertionError("unreachable")


def arcgis_geojson(layer: str) -> dict:
    """All features of a feature layer as one GeoJSON FeatureCollection, paging if needed."""
    base = f"{ARCGIS}/{layer}/FeatureServer/0/query"
    features: list[dict] = []
    offset = 0
    while True:
        params = {
            "where": "1=1",
            "outFields": "*",
            "outSR": "4326",
            "f": "geojson",
            "resultOffset": str(offset),
            "resultRecordCount": "1000",
            "orderByFields": "OBJECTID",
        }
        page = json.loads(request(f"{base}?{urllib.parse.urlencode(params)}"))
        batch = page.get("features", [])
        features.extend(batch)
        exceeded = page.get("properties", {}).get("exceededTransferLimit") or page.get("exceededTransferLimit")
        if not batch or not exceeded:
            break
        offset += len(batch)
    return {"type": "FeatureCollection", "features": features}


SOURCES = {
    "sustainability_explorer.geojson": {
        "what": "UMass Sustainability Explorer (2018), ArcGIS feature layer, 144 points",
        "url": f"{ARCGIS}/UMass_Sustainability_Explorer_2018/FeatureServer/0",
        "license": "none stated; UMass-owned. Do not republish; paraphrase and link.",
        "fetch": lambda: json.dumps(arcgis_geojson("UMass_Sustainability_Explorer_2018")).encode(),
    },
    "energy_benchmarking.geojson": {
        "what": "UMass Energy Benchmarking Explorer V3, ArcGIS feature layer, building footprints",
        "url": f"{ARCGIS}/UMass_Energy_Benchmarking_Explorer_V3/FeatureServer/0",
        "license": "none stated; UMass-owned. Used for footprints, building ids and facts.",
        "fetch": lambda: json.dumps(arcgis_geojson("UMass_Energy_Benchmarking_Explorer_V3")).encode(),
    },
    "osm.json": {
        "what": "OpenStreetMap via the Overpass API: solar, EV charging, artwork, named buildings, Wikidata-tagged objects",
        "url": "https://overpass-api.de/api/interpreter",
        "license": "ODbL 1.0. Attribute: (c) OpenStreetMap contributors.",
        "fetch": lambda: request(
            "https://overpass-api.de/api/interpreter",
            data=urllib.parse.urlencode({"data": OVERPASS_QUERY}).encode(),
        ),
    },
    "wikidata.json": {
        "what": "Wikidata items located in, part of, or owned by UMass Amherst (Q15142), with coordinates",
        "url": "https://query.wikidata.org/sparql",
        "license": "CC0 (Wikidata). Commons images are licensed per file.",
        "fetch": lambda: request(
            "https://query.wikidata.org/sparql?" + urllib.parse.urlencode({"query": WIKIDATA_QUERY, "format": "json"}),
            accept="application/sparql-results+json",
        ),
    },
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, help="output directory (default raw/<today>)")
    args = parser.parse_args()

    out = args.out or HERE / "raw" / dt.date.today().isoformat()
    out.mkdir(parents=True, exist_ok=True)
    manifest = {"fetched_at": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"), "sources": {}}

    failed = []
    for filename, src in SOURCES.items():
        print(f"fetching {filename} ...", file=sys.stderr)
        try:
            body = src["fetch"]()
        except Exception as err:  # noqa: BLE001 - report and carry on with the others
            print(f"  FAILED: {err}", file=sys.stderr)
            failed.append(filename)
            continue
        (out / filename).write_bytes(body)
        manifest["sources"][filename] = {k: v for k, v in src.items() if k != "fetch"} | {"bytes": len(body)}
        print(f"  {len(body):,} bytes", file=sys.stderr)

    (out / "sources.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"snapshot written to {out}", file=sys.stderr)
    if failed:
        print(f"failed: {', '.join(failed)}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
