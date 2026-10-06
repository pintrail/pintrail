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
    (root || document).querySelectorAll("[data-map]:not([data-overview-map])").forEach(function (el) {
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
        });

        // A "clear location" control blanks the fields so the artifact inherits
        // its parent's coordinates -- the indoor case.
        const clearBtn = document.querySelector(el.dataset.clearButton);
        if (clearBtn) {
          clearBtn.addEventListener("click", function () {
            if (marker) { map.removeLayer(marker); marker = null; }
            latIn.value = ""; lngIn.value = "";
          });
        }

        // Typing (or pasting) coordinates moves the pin, so an author can
        // check a pasted position against the map before saving. A pair
        // pasted whole into the latitude box, the way Google Maps copies one,
        // is split across both boxes.
        function fromInputs() {
          const pair = latIn.value.split(",");
          if (pair.length === 2 && lngIn.value.trim() === "") {
            latIn.value = pair[0].trim();
            lngIn.value = pair[1].trim();
          }
          const la = parseFloat(latIn.value), ln = parseFloat(lngIn.value);
          const ok = isFinite(la) && isFinite(ln) && Math.abs(la) <= 90 && Math.abs(ln) <= 180
            && /^\s*-?[\d.]+\s*$/.test(latIn.value) && /^\s*-?[\d.]+\s*$/.test(lngIn.value);
          if (ok) {
            place([la, ln]);
            map.setView([la, ln], Math.max(map.getZoom(), 17));
          } else if (latIn.value.trim() === "" && lngIn.value.trim() === "" && marker) {
            map.removeLayer(marker); marker = null;
          }
        }
        latIn.addEventListener("input", fromInputs);
        lngIn.addEventListener("input", fromInputs);
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

  // --- artifact form ------------------------------------------------------
  // Shows the help line for the chosen kind, and flags the location as
  // required when the artifact has no parent.
  function initForms(root) {
    root.querySelectorAll("[data-artifact-form]").forEach(function (form) {
      if (form.dataset.ready) return;
      form.dataset.ready = "1";

      const kind = form.querySelector("[data-kind-select]");
      const help = form.querySelector("[data-kind-help]");
      function showKind() {
        help.querySelectorAll("[data-for]").forEach(function (s) {
          s.classList.toggle("on", s.dataset.for === kind.value);
        });
      }
      kind.addEventListener("change", showKind);
      showKind();

      const parent = form.querySelector("[data-parent-select]");
      function showParent() { form.classList.toggle("top-level", parent.value === ""); }
      parent.addEventListener("change", showParent);
      showParent();
    });

    root.querySelectorAll("[data-tag-input]").forEach(initTagInput);
  }

  // A chip-style tag field. The hidden input carries "a, b, c" to the server,
  // which does the real cleaning; this only makes entry pleasant.
  function initTagInput(box) {
    if (box.dataset.ready) return;
    box.dataset.ready = "1";
    const list = box.querySelector(".chips");
    const text = box.querySelector("input[type=text]");
    const hidden = box.querySelector("input[type=hidden]");
    let tags = hidden.value.split(",").map(function (t) { return t.trim(); }).filter(Boolean);

    function sync() {
      hidden.value = tags.join(", ");
      list.replaceChildren();
      tags.forEach(function (t, i) {
        const li = document.createElement("li");
        li.className = "chip";
        li.textContent = t;
        const x = document.createElement("button");
        x.type = "button";
        x.textContent = "×";
        x.setAttribute("aria-label", "Remove tag " + t);
        x.addEventListener("click", function () { tags.splice(i, 1); sync(); text.focus(); });
        li.appendChild(x);
        list.appendChild(li);
      });
    }
    function commit() {
      text.value.split(",").forEach(function (raw) {
        const t = raw.trim().replace(/\s+/g, " ");
        if (t && !tags.some(function (x) { return x.toLowerCase() === t.toLowerCase(); })) tags.push(t);
      });
      text.value = "";
      sync();
    }
    text.addEventListener("keydown", function (e) {
      if (e.key === "Enter" || e.key === ",") {
        e.preventDefault();
        commit();
      } else if (e.key === "Backspace" && text.value === "" && tags.length) {
        tags.pop();
        sync();
      }
    });
    // Picking a suggestion from the datalist fires input with the full value.
    text.addEventListener("input", function (e) {
      if (e.inputType === "insertReplacementText" || e.inputType === undefined) commit();
    });
    text.addEventListener("blur", commit);
    box.addEventListener("click", function (e) { if (e.target === box) text.focus(); });
    // A tag typed but not yet Entered still counts when the form is saved.
    // Capture phase, so this runs before htmx reads the form.
    box.closest("form").addEventListener("submit", commit, true);
    sync();
  }

  // --- links -------------------------------------------------------------
  document.addEventListener("click", function (e) {
    const edit = e.target.closest("[data-edit-link]");
    if (edit) {
      const li = edit.closest(".link");
      li.classList.add("editing");
      const input = li.querySelector(".link-edit input[name=url]");
      if (input) input.focus();
      return;
    }
    const cancel = e.target.closest("[data-cancel-edit]");
    if (cancel) {
      const li = cancel.closest(".link");
      li.classList.remove("editing");
      li.querySelector(".link-edit").reset();
      const btn = li.querySelector("[data-edit-link]");
      if (btn) btn.focus();
    }
  });

  // Drag the grip to reorder; arrow keys on a focused grip do the same.
  // Either way the full new order is posted and the card re-renders.
  function initSortable(root) {
    root.querySelectorAll("[data-sortable]").forEach(function (list) {
      if (list.dataset.ready) return;
      list.dataset.ready = "1";
      let dragged = null;

      function save() {
        const ids = Array.from(list.children).map(function (li) { return li.dataset.id; });
        htmx.ajax("POST", list.dataset.sortable, {
          target: "#links", swap: "innerHTML",
          values: { csrf_token: list.dataset.csrf, ids: ids.join(",") },
        });
      }
      function clearMarks() {
        list.querySelectorAll(".drop-before, .drop-after").forEach(function (li) {
          li.classList.remove("drop-before", "drop-after");
        });
      }

      list.querySelectorAll(".grip").forEach(function (grip) {
        const li = grip.closest(".link");
        // Only the grip starts a drag, so text in the card stays selectable.
        grip.addEventListener("pointerdown", function () { li.draggable = true; });
        grip.addEventListener("keydown", function (e) {
          if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
          e.preventDefault();
          const sib = e.key === "ArrowUp" ? li.previousElementSibling : li.nextElementSibling;
          if (!sib) return;
          if (e.key === "ArrowUp") list.insertBefore(li, sib); else list.insertBefore(sib, li);
          grip.focus();
          clearTimeout(list._saveTimer);
          list._saveTimer = setTimeout(save, 400);
        });
      });

      list.addEventListener("dragstart", function (e) {
        dragged = e.target.closest(".link");
        if (!dragged) return;
        dragged.classList.add("dragging");
        e.dataTransfer.effectAllowed = "move";
        e.dataTransfer.setData("text/plain", dragged.dataset.id);
      });
      list.addEventListener("dragover", function (e) {
        if (!dragged) return;
        e.preventDefault();
        const over = e.target.closest(".link");
        clearMarks();
        if (!over || over === dragged) return;
        const r = over.getBoundingClientRect();
        over.classList.add(e.clientY < r.top + r.height / 2 ? "drop-before" : "drop-after");
      });
      list.addEventListener("drop", function (e) {
        if (!dragged) return;
        e.preventDefault();
        const over = e.target.closest(".link");
        if (over && over !== dragged) {
          const r = over.getBoundingClientRect();
          list.insertBefore(dragged, e.clientY < r.top + r.height / 2 ? over : over.nextSibling);
          save();
        }
      });
      list.addEventListener("dragend", function () {
        if (dragged) { dragged.classList.remove("dragging"); dragged.draggable = false; }
        dragged = null;
        clearMarks();
      });
    });
  }

  // --- map of every artifact ----------------------------------------------
  const KIND_COLORS = {
    building: "#2563eb", room: "#7c3aed", artwork: "#db2777",
    installation: "#059669", rooftop: "#d97706", other: "#64748b",
  };

  function openArtifact(id) {
    const url = "/studio/artifacts/" + id;
    htmx.ajax("GET", url, { target: "#detail", swap: "innerHTML" }).then(function () {
      history.pushState({ htmx: true }, "", url);
    });
  }

  function chip(text, cls) {
    const s = document.createElement("span");
    s.className = cls;
    s.textContent = text;
    return s;
  }

  function popupFor(m) {
    const div = document.createElement("div");
    div.className = "pin-popup";
    const h = document.createElement("h4");
    const a = document.createElement("a");
    a.textContent = m.name || "(untitled)";
    a.addEventListener("click", function () { openArtifact(m.id); });
    h.appendChild(a);
    div.appendChild(h);
    div.appendChild(chip(m.kind, "kind-chip"));
    if (m.tags.length) {
      const t = document.createElement("div");
      t.className = "tags";
      m.tags.forEach(function (x) { t.appendChild(chip(x, "tag")); });
      div.appendChild(t);
    }
    if (m.inside.length) {
      const ul = document.createElement("ul");
      m.inside.forEach(function (c) {
        const li = document.createElement("li");
        const ca = document.createElement("a");
        ca.textContent = c.name || "(untitled)";
        ca.addEventListener("click", function () { openArtifact(c.id); });
        li.appendChild(ca);
        li.appendChild(document.createTextNode(" "));
        li.appendChild(chip(c.kind, "kind-chip"));
        ul.appendChild(li);
      });
      const label = document.createElement("div");
      label.className = "hint";
      label.textContent = "Inside (" + m.inside.length + "):";
      div.appendChild(label);
      div.appendChild(ul);
    }
    return div;
  }

  function initOverview(root) {
    root.querySelectorAll("[data-overview-map]").forEach(function (el) {
      if (el._leaflet_id) return;
      const card = el.closest(".map-card");
      const data = JSON.parse(card.querySelector("[data-map-data]").textContent);
      const map = L.map(el).setView(CAMPUS, 15);
      L.tileLayer("https://tile.openstreetmap.org/{z}/{x}/{y}.png", {
        maxZoom: 19, attribution: "© OpenStreetMap",
      }).addTo(map);

      const entries = data.markers.map(function (m) {
        const color = KIND_COLORS[m.kind] || KIND_COLORS.other;
        const layer = L.circleMarker([m.lat, m.lng], {
          radius: m.inside.length ? 10 : 8, color: "#fff", weight: 2,
          fillColor: color, fillOpacity: 0.9,
        }).bindPopup(popupFor(m)).bindTooltip(m.name || "(untitled)");
        return { m: m, layer: layer };
      });

      const kindSel = card.querySelector('[data-map-filter="kind"]');
      const tagSel = card.querySelector('[data-map-filter="tag"]');
      const count = card.querySelector("[data-map-count]");

      function matches(m) {
        const all = [m].concat(m.inside);
        const k = kindSel.value, t = tagSel.value;
        return all.some(function (a) {
          return (!k || a.kind === k) &&
            (!t || a.tags.some(function (x) { return x.toLowerCase() === t; }));
        });
      }
      function apply(fit) {
        const shown = [];
        entries.forEach(function (e) {
          if (matches(e.m)) { e.layer.addTo(map); shown.push(e.layer); }
          else map.removeLayer(e.layer);
        });
        count.textContent = shown.length;
        if (fit && shown.length) {
          map.fitBounds(L.featureGroup(shown).getBounds().pad(0.2), { maxZoom: 17 });
        }
      }
      kindSel.addEventListener("change", function () { apply(true); });
      tagSel.addEventListener("change", function () { apply(true); });
      apply(true);
      setTimeout(function () { map.invalidateSize(); }, 50);
    });
  }

  function enhance(root) {
    root = root || document;
    initMaps(root);
    initForms(root);
    initSortable(root);
    initOverview(root);
  }

  document.addEventListener("DOMContentLoaded", function () { enhance(document); });
  document.body.addEventListener("htmx:afterSettle", function (e) { enhance(e.target); });
})();
