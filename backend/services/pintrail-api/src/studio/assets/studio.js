// Studio front-end glue. Two jobs htmx cannot do on its own:
//   1. Initialise Leaflet maps inside fragments htmx swaps in.
//   2. Drive the three-step direct-to-storage upload (intent -> PUT -> complete).
// Everything else is plain htmx attributes in the templates.

(function () {
  "use strict";

  // UMass Amherst, the default campus view when an artifact has no location.
  const CAMPUS = [42.3912, -72.5262];

  // Initialises every map container that has not been set up yet. Runs on load
  // and after each htmx swap, so a detail or form fragment gets its map without
  // relying on <script> execution inside swapped content.
  function initMaps(root) {
    (root || document).querySelectorAll("[data-map]").forEach(function (el) {
      if (el._leaflet_id) return; // already initialised

      const hasLoc = el.dataset.lat !== "" && el.dataset.lat != null;
      const lat = hasLoc ? parseFloat(el.dataset.lat) : CAMPUS[0];
      const lng = hasLoc ? parseFloat(el.dataset.lng) : CAMPUS[1];
      const editable = el.dataset.editable === "true";

      const map = L.map(el, { scrollWheelZoom: editable }).setView(
        [lat, lng],
        hasLoc ? 17 : 15
      );
      L.tileLayer("https://tile.openstreetmap.org/{z}/{x}/{y}.png", {
        maxZoom: 19,
        attribution: "© OpenStreetMap",
      }).addTo(map);

      // A circle marker rather than the default pin, so no marker image assets
      // are needed -- keeps the vendored set to htmx + Leaflet only.
      let marker = null;
      function place(latlng) {
        if (marker) marker.setLatLng(latlng);
        else marker = L.circleMarker(latlng, {
          radius: 8, color: "#2563eb", fillColor: "#2563eb", fillOpacity: 0.7,
        }).addTo(map);
      }
      if (hasLoc) place([lat, lng]);

      if (editable) {
        const latIn = document.querySelector(el.dataset.inputLat);
        const lngIn = document.querySelector(el.dataset.inputLng);

        map.on("click", function (e) {
          place(e.latlng);
          // 6 decimals ~= 11cm, far finer than GPS; more is noise.
          latIn.value = e.latlng.lat.toFixed(6);
          lngIn.value = e.latlng.lng.toFixed(6);
          latIn.dispatchEvent(new Event("input", { bubbles: true }));
        });

        // A "clear location" control blanks the fields so the artifact inherits
        // its parent's coordinates -- the indoor case.
        const clearBtn = document.querySelector(el.dataset.clearButton);
        if (clearBtn) {
          clearBtn.addEventListener("click", function () {
            if (marker) { map.removeLayer(marker); marker = null; }
            latIn.value = ""; lngIn.value = "";
            latIn.dispatchEvent(new Event("input", { bubbles: true }));
          });
        }
      }

      // A fragment swapped into a hidden or zero-size container measures wrong;
      // recompute once it is visible.
      setTimeout(function () { map.invalidateSize(); }, 50);
    });
  }

  // --- media upload --------------------------------------------------------
  // Reuses the existing cookie-authed JSON endpoints the mobile app uses:
  //   POST /artifacts/{id}/attachments/upload-intent -> presigned PUT URL
  //   PUT  <that URL>                                 -> bytes go to storage
  //   POST /attachments/{id}/complete                -> hands it to the worker
  // Then htmx polls the gallery fragment until the worker produces a thumbnail.
  async function uploadFiles(artifactId, files, statusEl) {
    for (const file of files) {
      statusEl.textContent = "Uploading " + file.name + "…";
      try {
        const intentRes = await fetch(
          "/artifacts/" + artifactId + "/attachments/upload-intent",
          {
            method: "POST",
            credentials: "same-origin",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ filename: file.name, mime_type: file.type }),
          }
        );
        if (!intentRes.ok) {
          statusEl.textContent = (await intentRes.json()).error || "Upload rejected.";
          continue;
        }
        const intent = await intentRes.json();

        const putRes = await fetch(intent.upload_url, {
          method: "PUT",
          headers: { "content-type": intent.required_headers["content-type"] },
          body: file,
        });
        if (!putRes.ok) { statusEl.textContent = "Storage rejected the upload."; continue; }

        const doneRes = await fetch(
          "/attachments/" + intent.attachment_id + "/complete",
          { method: "POST", credentials: "same-origin" }
        );
        if (!doneRes.ok) {
          statusEl.textContent = (await doneRes.json()).error || "Could not finalise.";
          continue;
        }
      } catch (err) {
        statusEl.textContent = "Upload failed: " + err;
        continue;
      }
    }
    statusEl.textContent = "";
    // Tell the gallery to refresh; it polls until processing finishes.
    document.body.dispatchEvent(new CustomEvent("refresh-attachments"));
  }

  // Wired via event delegation so it survives htmx swaps.
  document.addEventListener("change", function (e) {
    const input = e.target.closest("input[type=file][data-upload-for]");
    if (!input || !input.files.length) return;
    const status = document.querySelector(input.dataset.statusTarget);
    uploadFiles(input.dataset.uploadFor, input.files, status);
    input.value = "";
  });

  document.addEventListener("DOMContentLoaded", function () { initMaps(document); });
  document.body.addEventListener("htmx:afterSettle", function (e) { initMaps(e.target); });
})();
