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
      if (hasLoc && !editable) place([lat, lng]);

      if (editable) initEditableMap(el, map, hasLoc ? [lat, lng] : null);

      // A fragment swapped into a hidden or zero-size container measures wrong;
      // recompute once it is visible.
      setTimeout(function () { map.invalidateSize(); }, 50);
    });
  }

  // --- the location picker in the artifact form -------------------------------
  //
  // Two kinds of pin. A solid one is this artifact's own location, and its
  // coordinates are in the boxes. A hollow, dashed one is the location it
  // inherits from its parent: shown so the author starts from the right spot,
  // with the coordinates only as placeholders, because saving them would stop
  // the artifact following its parent if the parent's pin is later corrected.
  // Dragging the hollow pin, or clicking the map, turns it into its own.
  function pinIcon(cls) {
    return L.divIcon({ className: "pin " + cls, iconSize: [22, 22], iconAnchor: [11, 11] });
  }

  function initEditableMap(el, map, initial) {
    const latIn = document.querySelector(el.dataset.inputLat);
    const lngIn = document.querySelector(el.dataset.inputLng);
    const parentSel = document.querySelector(el.dataset.parentSelect);
    const note = document.querySelector(el.dataset.inheritNote);
    let own = null, ghost = null, accuracy = null;
    function dropAccuracy() { if (accuracy) { map.removeLayer(accuracy); accuracy = null; } }

    // 6 decimals ~= 11cm, far finer than GPS; more is noise.
    function write(ll) {
      latIn.value = ll.lat.toFixed(6);
      lngIn.value = ll.lng.toFixed(6);
    }
    function say(text) {
      if (!note) return;
      note.textContent = text || "";
      note.hidden = !text;
    }
    function dropGhost() { if (ghost) { map.removeLayer(ghost); ghost = null; } }
    function dropOwn() { if (own) { map.removeLayer(own); own = null; } }

    function inherited() {
      const opt = parentSel && parentSel.selectedOptions[0];
      if (!opt || !opt.dataset.lat) return null;
      return {
        latlng: L.latLng(parseFloat(opt.dataset.lat), parseFloat(opt.dataset.lng)),
        source: opt.dataset.source || "the parent",
      };
    }

    function setOwn(ll) {
      dropGhost();
      if (own) own.setLatLng(ll);
      else {
        own = L.marker(ll, { icon: pinIcon("pin-own"), draggable: true, keyboard: false }).addTo(map);
        own.on("dragend", function () { dropAccuracy(); write(own.getLatLng()); });
      }
      const inh = inherited();
      say(inh ? "This artifact has its own location. Use “Clear” to go back to " + inh.source + "’s." : "");
    }

    function showInherited(pan) {
      if (own) return;
      const inh = inherited();
      dropGhost();
      if (!inh) {
        latIn.placeholder = "latitude";
        lngIn.placeholder = "longitude";
        say("");
        return;
      }
      latIn.placeholder = inh.latlng.lat.toFixed(6);
      lngIn.placeholder = inh.latlng.lng.toFixed(6);
      ghost = L.marker(inh.latlng, { icon: pinIcon("pin-inherited"), draggable: true, keyboard: false })
        .bindTooltip("Location of " + inh.source)
        .addTo(map);
      ghost.on("dragend", function () {
        const ll = ghost.getLatLng();
        setOwn(ll);
        write(ll);
      });
      say("Showing the location of " + inh.source + ", which this artifact uses. " +
          "To give it a spot of its own, drag the pin or click the map.");
      if (pan) map.setView(inh.latlng, 18);
    }

    if (initial) setOwn(L.latLng(initial[0], initial[1]));
    else showInherited(true);

    map.on("click", function (e) {
      dropAccuracy();
      setOwn(e.latlng);
      write(e.latlng);
    });

    // "Clear" blanks the boxes so the artifact goes back to inheriting.
    const clearBtn = document.querySelector(el.dataset.clearButton);
    if (clearBtn) {
      clearBtn.addEventListener("click", function () {
        dropAccuracy();
        dropOwn();
        latIn.value = ""; lngIn.value = "";
        showInherited(true);
      });
    }

    if (parentSel) {
      parentSel.addEventListener("change", function () {
        if (own) setOwn(own.getLatLng()); // refresh the note's parent name
        else showInherited(true);
      });
    }

    // Typing (or pasting) coordinates moves the pin, so an author can check a
    // pasted position against the map before saving. A pair pasted whole
    // into the latitude box, the way Google Maps copies one, is split across
    // both boxes.
    function fromInputs() {
      const pair = latIn.value.split(",");
      if (pair.length === 2 && lngIn.value.trim() === "") {
        latIn.value = pair[0].trim();
        lngIn.value = pair[1].trim();
      }
      const num = /^\s*-?\d+(\.\d+)?\s*$/;
      const la = parseFloat(latIn.value), ln = parseFloat(lngIn.value);
      if (num.test(latIn.value) && num.test(lngIn.value) && Math.abs(la) <= 90 && Math.abs(ln) <= 180) {
        setOwn(L.latLng(la, ln));
        map.setView([la, ln], Math.max(map.getZoom(), 17));
      } else if (latIn.value.trim() === "" && lngIn.value.trim() === "") {
        dropOwn();
        showInherited(false);
      }
    }
    latIn.addEventListener("input", function () { dropAccuracy(); fromInputs(); });
    lngIn.addEventListener("input", function () { dropAccuracy(); fromInputs(); });

    // "Use my location": the device's GPS position. A phone's first fix is
    // often coarse, so it keeps listening for a few seconds and keeps the
    // most accurate reading, stopping early once it is within 10 m. The
    // circle shows how far off the reading may be; the author still checks
    // the pin and can drag it.
    const locateBtn = document.querySelector(el.dataset.locateButton);
    const status = document.querySelector(el.dataset.locateStatus);
    function report(text, isError) {
      if (!status) return;
      status.textContent = text;
      status.classList.toggle("err", !!isError);
    }
    if (locateBtn) {
      locateBtn.addEventListener("click", function () {
        if (!navigator.geolocation) {
          report("This browser can't share its location. Place the pin on the map instead.", true);
          return;
        }
        if (!window.isSecureContext) {
          report("Location only works over https. Place the pin on the map instead.", true);
          return;
        }
        let best = null, watchId = null, timer = null;
        const started = Date.now();
        locateBtn.disabled = true;
        report("Finding your location…");

        function finish() {
          if (watchId !== null) navigator.geolocation.clearWatch(watchId);
          clearTimeout(timer);
          locateBtn.disabled = false;
          if (best) {
            const m = Math.round(best.coords.accuracy);
            report("Pin placed at your location, accurate to about " + m + " m. " +
              (m > 30 ? "That's rough (common indoors): drag the pin to the exact spot. "
                      : "Check it's on the right spot, and drag it if not. ") +
              "Tap the button again to retry.");
          }
        }
        function use(pos) {
          const ll = L.latLng(pos.coords.latitude, pos.coords.longitude);
          setOwn(ll);
          write(ll);
          if (accuracy) accuracy.setLatLng(ll).setRadius(pos.coords.accuracy);
          else accuracy = L.circle(ll, {
            radius: pos.coords.accuracy, color: "#2563eb", weight: 1,
            fillColor: "#2563eb", fillOpacity: 0.12, interactive: false,
          }).addTo(map);
          map.setView(ll, Math.max(map.getZoom(), 18));
        }

        watchId = navigator.geolocation.watchPosition(function (pos) {
          if (!best || pos.coords.accuracy < best.coords.accuracy) {
            best = pos;
            use(pos);
            report("Finding your location… accurate to about " + Math.round(pos.coords.accuracy) + " m so far.");
          }
          if (best.coords.accuracy <= 10 || Date.now() - started > 12000) finish();
        }, function (err) {
          if (best) { finish(); return; }
          if (watchId !== null) navigator.geolocation.clearWatch(watchId);
          clearTimeout(timer);
          locateBtn.disabled = false;
          const why = {
            1: "Location access is blocked. Allow it for this site in your browser settings, or place the pin on the map.",
            2: "Your location isn't available right now. Try again outside, or place the pin on the map.",
            3: "Finding your location took too long. Try again, or place the pin on the map.",
          }[err.code] || "Couldn't get your location. Place the pin on the map instead.";
          report(why, true);
        }, { enableHighAccuracy: true, maximumAge: 0, timeout: 20000 });
        timer = setTimeout(finish, 15000);
      });
    }
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

      // A topic's form has neither a kind nor a parent.
      const kind = form.querySelector("[data-kind-select]");
      const help = form.querySelector("[data-kind-help]");
      if (kind && help) {
        const showKind = function () {
          help.querySelectorAll("[data-for]").forEach(function (s) {
            s.classList.toggle("on", s.dataset.for === kind.value);
          });
        };
        kind.addEventListener("change", showKind);
        showKind();
      }

      const parent = form.querySelector("[data-parent-select]");
      if (parent) {
        const showParent = function () { form.classList.toggle("top-level", parent.value === ""); };
        parent.addEventListener("change", showParent);
        showParent();
      }
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
      const input = li.querySelector(".link-edit input:not([type=hidden]), .link-edit textarea");
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
          target: list.dataset.sortableTarget || "#links", swap: "innerHTML",
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

  const STATUS_COLORS = { draft: "#94a3b8", ready: "#d97706", approved: "#16a34a" };
  const STATUS_LABELS = { draft: "Draft", ready: "Ready for review", approved: "Approved" };

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
    div.appendChild(chip(STATUS_LABELS[m.status] || m.status, "status-chip status-" + m.status));
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

  // --- a trail's route ---------------------------------------------------------
  // Numbered pins in walking order, joined by a line. A topic's page uses the
  // same map for its linked artifacts, unnumbered and with no line
  // (`route: false`).
  function initTrailMaps(root) {
    root.querySelectorAll("[data-trail-map]").forEach(function (el) {
      if (el._leaflet_id) return;
      const data = JSON.parse(el.parentElement.querySelector("[data-trail-data]").textContent);
      const map = L.map(el).setView(CAMPUS, 15);
      L.tileLayer("https://tile.openstreetmap.org/{z}/{x}/{y}.png", {
        maxZoom: 19, attribution: "© OpenStreetMap",
      }).addTo(map);
      const pts = data.points;
      // Stops inside the same building share a spot. Fan their numbers out
      // sideways on screen (not on the map) so each stays readable at any zoom.
      const total = {}, seen = {};
      pts.forEach(function (p) { const k = p.lat.toFixed(6) + "," + p.lng.toFixed(6); total[k] = (total[k] || 0) + 1; });
      const layers = pts.map(function (p) {
        const key = p.lat.toFixed(6) + "," + p.lng.toFixed(6);
        const i = seen[key] = (seen[key] || 0) + 1;
        const shift = (i - 1 - (total[key] - 1) / 2) * 28;
        const label = p.n == null ? p.name : p.n + ". " + p.name;
        const icon = p.n == null
          ? L.divIcon({ className: "stop-pin topic-pin", html: "", iconSize: [18, 18], iconAnchor: [9 - shift * 0.7, 9] })
          : L.divIcon({ className: "stop-pin", html: "<span>" + p.n + "</span>", iconSize: [26, 26], iconAnchor: [13 - shift, 13] });
        return L.marker([p.lat, p.lng], { icon: icon, keyboard: false, title: label })
          .bindTooltip(label)
          .on("click", function () { openArtifact(p.artifact_id); });
      });
      if (data.route !== false && pts.length > 1) {
        L.polyline(layers.map(function (m) { return m.getLatLng(); }), {
          color: "#2563eb", weight: 3, opacity: 0.7, dashArray: "6 6",
        }).addTo(map);
      }
      layers.forEach(function (m) { m.addTo(map); });
      if (layers.length) map.fitBounds(L.featureGroup(layers).getBounds().pad(0.25), { maxZoom: 18 });
      setTimeout(function () { map.invalidateSize(); }, 50);
    });
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
          radius: m.inside.length ? 10 : 8, color: STATUS_COLORS[m.status] || "#fff", weight: 3,
          fillColor: color, fillOpacity: 0.9,
        }).bindPopup(popupFor(m)).bindTooltip(m.name || "(untitled)");
        return { m: m, layer: layer };
      });

      const kindSel = card.querySelector('[data-map-filter="kind"]');
      const tagSel = card.querySelector('[data-map-filter="tag"]');
      const statusSel = card.querySelector('[data-map-filter="status"]');
      const count = card.querySelector("[data-map-count]");

      function matches(m) {
        const all = [m].concat(m.inside);
        const k = kindSel.value, t = tagSel.value, st = statusSel.value;
        return all.some(function (a) {
          return (!k || a.kind === k) && (!st || a.status === st) &&
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
      statusSel.addEventListener("change", function () { apply(true); });
      apply(true);
      setTimeout(function () { map.invalidateSize(); }, 50);
    });
  }

  // --- artifact list as a drawer on phones -----------------------------------
  function setNav(open) {
    document.body.classList.toggle("nav-open", open);
    const t = document.querySelector("[data-nav-toggle]");
    if (t) t.setAttribute("aria-expanded", open ? "true" : "false");
  }
  document.addEventListener("click", function (e) {
    if (e.target.closest("[data-nav-toggle]")) { setNav(!document.body.classList.contains("nav-open")); return; }
    if (e.target.closest("[data-nav-close]")) { setNav(false); return; }
    // Picking an artifact, Map, or + New in the drawer shows it, so close.
    if (e.target.closest(".sidebar a.node, .sidebar .head button")) setNav(false);
  });
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape" && document.body.classList.contains("nav-open")) setNav(false);
  });

  // --- profile photo ------------------------------------------------------
  // Sent as the raw request body; the server crops, resizes, and re-encodes it.
  document.addEventListener("change", async function (e) {
    const input = e.target.closest("[data-avatar-input]");
    if (!input || !input.files.length) return;
    const file = input.files[0];
    const box = input.closest("#profile-photo");
    const status = box.querySelector("[data-avatar-status]");
    status.classList.remove("err");
    status.textContent = "Uploading…";
    try {
      const res = await fetch("/studio/profile/photo", {
        method: "POST",
        headers: { "X-CSRF-Token": input.dataset.csrf, "Content-Type": file.type || "application/octet-stream" },
        body: file,
      });
      if (!res.ok) {
        let msg = "Upload failed (" + res.status + ").";
        try { msg = (await res.json()).error || msg; } catch (_) {}
        if (res.status === 413) msg = "That file is too large. Use a photo under 20 MB.";
        throw new Error(msg);
      }
      const holder = document.createElement("div");
      holder.innerHTML = await res.text();
      const fresh = holder.firstElementChild;
      box.replaceWith(fresh);
      htmx.process(fresh);
      htmx.trigger(document.body, "profile-photo-changed");
    } catch (err) {
      status.classList.add("err");
      status.textContent = err.message;
      input.value = "";
    }
  });

  // --- copy buttons --------------------------------------------------------------
  document.addEventListener("click", function (e) {
    const b = e.target.closest("[data-copy]");
    if (!b) return;
    navigator.clipboard.writeText(b.dataset.copy).then(function () {
      const was = b.textContent;
      b.textContent = "Copied";
      setTimeout(function () { b.textContent = was; }, 1500);
    });
  });

  // --- times in the reader's own zone -------------------------------------
  const DATE_FMT = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });
  function localTimes(root) {
    root.querySelectorAll("time[data-local]").forEach(function (t) {
      const d = new Date(t.getAttribute("datetime"));
      if (!isNaN(d)) { t.textContent = DATE_FMT.format(d); t.title = d.toString(); }
    });
  }

  // --- tooltips ------------------------------------------------------------------
  // Anything with data-tip shows it in one floating box on hover or keyboard
  // focus. Positioned by script rather than CSS so a card's edge or the map
  // can't clip it.
  const tip = document.getElementById("tip-pop");
  let tipFor = null;
  function showTip(el) {
    if (!tip || !el.dataset.tip) return;
    tipFor = el;
    tip.textContent = el.dataset.tip;
    tip.hidden = false;
    const r = el.getBoundingClientRect(), t = tip.getBoundingClientRect(), pad = 8;
    let top = r.top - t.height - 8;
    if (top < pad) top = r.bottom + 8;
    let left = r.left + r.width / 2 - t.width / 2;
    left = Math.max(pad, Math.min(left, window.innerWidth - t.width - pad));
    tip.style.top = top + "px";
    tip.style.left = left + "px";
  }
  function hideTip() { if (tip) tip.hidden = true; tipFor = null; }
  if (tip) {
    document.addEventListener("mouseover", function (e) {
      const el = e.target.closest("[data-tip]");
      if (el && el !== tipFor) showTip(el);
      else if (!el && tipFor) hideTip();
    });
    document.addEventListener("focusin", function (e) {
      const el = e.target.closest("[data-tip]");
      if (el && el.matches(":focus-visible")) showTip(el); else hideTip();
    });
    document.addEventListener("focusout", hideTip);
    window.addEventListener("scroll", hideTip, true);
    document.addEventListener("click", hideTip, true);
  }

  // --- the help panel -------------------------------------------------------------
  // A "?" (data-help="page" or "page#heading") opens that manual page in a
  // panel over the right of the screen; links inside it stay in the panel.
  const panel = document.getElementById("help-panel");
  let panelOpener = null;
  function openHelp(target, opener) {
    if (!panel) return;
    const hash = target.indexOf("#") >= 0 ? target.slice(target.indexOf("#") + 1) : "";
    const page = target.split("#")[0];
    panelOpener = opener || document.activeElement;
    htmx.ajax("GET", "/studio/help/" + page + "?panel=1", { target: "#help-panel-body", swap: "innerHTML" }).then(function () {
      panel.hidden = false;
      const at = hash && document.getElementById("help-panel-body").querySelector("[id='" + hash + "']");
      panel.scrollTop = 0;
      if (at) at.scrollIntoView({ block: "start" });
      panel.focus({ preventScroll: true });
    });
  }
  function closeHelp() {
    if (!panel || panel.hidden) return;
    panel.hidden = true;
    if (panelOpener && document.body.contains(panelOpener)) panelOpener.focus();
  }
  document.addEventListener("click", function (e) {
    const q = e.target.closest("[data-help]");
    if (q) { e.preventDefault(); openHelp(q.dataset.help, q); return; }
    if (e.target.closest("[data-help-close]")) { closeHelp(); return; }
    const navBtn = e.target.closest("[data-help-nav-toggle]");
    if (navBtn) {
      const open = navBtn.parentElement.classList.toggle("open");
      navBtn.setAttribute("aria-expanded", open ? "true" : "false");
      return;
    }
    // Inside the panel, a link to another manual page loads in the panel.
    const a = e.target.closest("#help-panel-body [data-help-panel-links] a[href^='/studio/help/']");
    if (a) { e.preventDefault(); openHelp(a.getAttribute("href").replace("/studio/help/", ""), panelOpener); }
    // The panel stays open while the author works underneath it; the close
    // button or Escape shuts it.
  });

  // Keyboard: ? opens the manual, Escape closes the panel.
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape") { closeHelp(); hideTip(); return; }
    if (e.key !== "?" || e.ctrlKey || e.metaKey || e.altKey) return;
    const el = document.activeElement;
    if (el && (el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName))) return;
    e.preventDefault();
    htmx.ajax("GET", "/studio/help", { target: "#detail", swap: "innerHTML" }).then(function () {
      history.pushState({}, "", "/studio/help");
    });
  });

  // --- searching the manual ---------------------------------------------------------
  function initHelpSearch(root) {
    root.querySelectorAll("[data-help-search]").forEach(function (input) {
      if (input.dataset.ready) return;
      input.dataset.ready = "1";
      const card = input.closest(".help-hero");
      const pages = JSON.parse(card.querySelector("[data-help-index]").textContent);
      const out = card.querySelector("[data-help-results]");
      const sections = document.querySelector("[data-help-sections]");
      const esc = function (t) { return t.replace(/[&<>"]/g, function (c) { return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]; }); };
      function snippet(text, words) {
        const lower = text.toLowerCase();
        let at = -1;
        words.forEach(function (w) { const i = lower.indexOf(w); if (i >= 0 && (at < 0 || i < at)) at = i; });
        if (at < 0) return "";
        const start = Math.max(0, at - 60), end = Math.min(text.length, at + 140);
        let s = esc((start ? "…" : "") + text.slice(start, end) + (end < text.length ? "…" : ""));
        words.forEach(function (w) {
          s = s.replace(new RegExp("(" + w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + ")", "gi"), "<mark>$1</mark>");
        });
        return s;
      }
      function run() {
        const words = input.value.toLowerCase().split(/\s+/).filter(function (w) { return w.length > 1; });
        if (!words.length) { out.innerHTML = ""; sections.hidden = false; return; }
        const hits = pages.map(function (p) {
          const title = p.title.toLowerCase(), body = (p.summary + " " + p.text).toLowerCase();
          let score = 0;
          for (const w of words) {
            if (title.indexOf(w) >= 0) score += 10;
            else if (body.indexOf(w) >= 0) score += 1 + Math.min(4, body.split(w).length - 1) / 2;
            else return null;
          }
          return { p: p, score: score };
        }).filter(Boolean).sort(function (a, b) { return b.score - a.score; }).slice(0, 12);
        sections.hidden = true;
        out.innerHTML = hits.length
          ? '<ul class="help-results">' + hits.map(function (h) {
              return '<li><a href="/studio/help/' + h.p.slug + '">' + esc(h.p.title) + '</a> <span class="hint">' + esc(h.p.section) +
                '</span><span class="snippet">' + (snippet(h.p.text, words) || esc(h.p.summary)) + "</span></li>";
            }).join("") + "</ul>"
          : '<p class="hint">Nothing matches. Try another word, or browse the sections below.</p>';
        if (!hits.length) sections.hidden = false;
        htmx.process(out);
      }
      input.addEventListener("input", run);
      input.focus();
    });
  }

  function enhance(root) {
    root = root || document;
    localTimes(root);
    initMaps(root);
    initForms(root);
    initSortable(root);
    initOverview(root);
    initTrailMaps(root);
    initHelpSearch(root);
  }

  document.addEventListener("DOMContentLoaded", function () { enhance(document); });
  document.body.addEventListener("htmx:afterSettle", function (e) { enhance(e.target); });
})();
