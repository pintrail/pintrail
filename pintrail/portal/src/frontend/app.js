const state = {
  artifacts: [],
  currentArtifact: null,
  saveTimer: null,
  imagePollTimer: null,
  isLoadingForm: false,
  isDraggingImages: false,
};

const elements = {
  artifactList: document.getElementById('artifact-list'),
  childList: document.getElementById('child-list'),
  editorTitle: document.getElementById('editor-title'),
  editorSubtitle: document.getElementById('editor-subtitle'),
  saveStatus: document.getElementById('save-status'),
  newRootButton: document.getElementById('new-root-button'),
  refreshButton: document.getElementById('refresh-button'),
  newChildButton: document.getElementById('new-child-button'),
  deleteArtifactButton: document.getElementById('delete-artifact-button'),
  form: document.getElementById('artifact-form'),
  artifactId: document.getElementById('artifact-id'),
  parentId: document.getElementById('parent-id'),
  name: document.getElementById('name'),
  desc: document.getElementById('desc'),
  lat: document.getElementById('lat'),
  lng: document.getElementById('lng'),
  imageDropzone: document.getElementById('image-dropzone'),
  imageInput: document.getElementById('image-input'),
  imageGallery: document.getElementById('image-gallery'),
};

async function api(path, options = {}) {
  const headers = new Headers(options.headers || {});
  const isFormData = options.body instanceof FormData;

  if (!isFormData && !headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }

  const response = await fetch(path, {
    ...options,
    headers,
  });

  if (!response.ok) {
    const payload = await response.json().catch(() => ({}));
    throw new Error(payload.message || 'Request failed.');
  }

  return response.json();
}

function setStatus(message) {
  elements.saveStatus.textContent = message;
}

function displayName(artifact) {
  return artifact.name.trim() || 'Untitled Artifact';
}

function renderArtifactList() {
  if (!state.artifacts.length) {
    elements.artifactList.innerHTML =
      '<div class="empty-state">No artifacts yet. Create one to get started.</div>';
    return;
  }

  const markup = buildArtifactTree(null, 0, false);
  elements.artifactList.innerHTML = `<div class="artifact-tree">${markup}</div>`;

  for (const button of elements.artifactList.querySelectorAll('[data-artifact-id]')) {
    button.addEventListener('click', () => {
      void loadArtifact(button.dataset.artifactId);
    });
  }
}

function buildArtifactTree(parentId, depth, useChildSelector) {
  const children = state.artifacts.filter(artifact => artifact.parentId === parentId);

  return children
    .map(artifact => {
      const isActive = artifact.id === state.currentArtifact?.id ? 'active' : '';
      const childCount = state.artifacts.filter(item => item.parentId === artifact.id).length;
      const relationLabel = childCount
        ? `${childCount} nested artifact${childCount === 1 ? '' : 's'}`
        : artifact.parentId
          ? 'Child artifact'
          : 'Root artifact';

      return `
        <div class="tree-node depth-${depth}">
          <button
            class="${useChildSelector ? 'child-item' : 'artifact-item'} ${isActive}"
            ${useChildSelector ? `data-child-id="${artifact.id}"` : `data-artifact-id="${artifact.id}"`}
            style="--depth:${depth}"
          >
            <span class="${useChildSelector ? 'child-title-row' : 'artifact-title-row'}">
              <span class="artifact-branch-icon">${childCount ? '▾' : '•'}</span>
              <span class="${useChildSelector ? 'child-title' : 'artifact-title'}">${escapeHtml(displayName(artifact))}</span>
            </span>
            <span class="${useChildSelector ? 'child-meta' : 'artifact-meta'}">${escapeHtml(relationLabel)}</span>
          </button>
          ${childCount ? `<div class="tree-children">${buildArtifactTree(artifact.id, depth + 1, useChildSelector)}</div>` : ''}
        </div>
      `;
    })
    .join('');
}

function renderChildren() {
  if (!state.currentArtifact) {
    elements.childList.className = 'child-list empty-state';
    elements.childList.textContent = 'Select an artifact to see its nested artifacts.';
    return;
  }

  const markup = buildArtifactTree(state.currentArtifact.id, 0, true);
  if (!markup) {
    elements.childList.className = 'child-list empty-state';
    elements.childList.textContent = 'This artifact does not have any nested artifacts yet.';
    return;
  }

  elements.childList.className = 'child-list';
  elements.childList.innerHTML = `<div class="artifact-tree">${markup}</div>`;

  for (const button of elements.childList.querySelectorAll('[data-child-id]')) {
    button.addEventListener('click', () => {
      void loadArtifact(button.dataset.childId);
    });
  }
}

function renderForm() {
  const artifact = state.currentArtifact;
  state.isLoadingForm = true;

  if (!artifact) {
    elements.editorTitle.textContent = 'Create or select an artifact';
    elements.editorSubtitle.textContent =
      'New artifacts start empty with a unique ID, then fill in details here.';
    elements.newChildButton.disabled = true;
    elements.deleteArtifactButton.disabled = true;
    elements.form.reset();
    elements.artifactId.value = '';
    elements.parentId.value = '';
    renderImageSection();
    state.isLoadingForm = false;
    renderChildren();
    return;
  }

  elements.editorTitle.textContent = displayName(artifact);
  elements.editorSubtitle.textContent = artifact.parentId
    ? `Child artifact linked to parent ${artifact.parentId}.`
    : 'Root artifact with no parent.';
  elements.newChildButton.disabled = false;
  elements.deleteArtifactButton.disabled = false;
  elements.artifactId.value = artifact.id;
  elements.parentId.value = artifact.parentId ?? '';
  elements.name.value = artifact.name;
  elements.desc.value = artifact.desc;
  elements.lat.value = artifact.loc?.lat ?? '';
  elements.lng.value = artifact.loc?.lng ?? '';
  renderImageSection();
  state.isLoadingForm = false;
  renderChildren();
}

function renderImageSection() {
  const artifact = state.currentArtifact;
  const hasArtifact = Boolean(artifact);

  elements.imageDropzone.classList.toggle('disabled', !hasArtifact);
  elements.imageDropzone.classList.toggle('dragging', state.isDraggingImages && hasArtifact);
  elements.imageDropzone.setAttribute('aria-disabled', hasArtifact ? 'false' : 'true');

  if (!artifact) {
    elements.imageDropzone.innerHTML = `
      <p class="dropzone-title">Attach images</p>
      <p class="dropzone-copy">Select an artifact first, then drop images here.</p>
    `;
    elements.imageGallery.innerHTML =
      '<div class="empty-state">Processed images will appear here once an artifact is selected.</div>';
    stopImagePolling();
    return;
  }

  elements.imageDropzone.innerHTML = `
    <p class="dropzone-title">Drop images here</p>
    <p class="dropzone-copy">Drag and drop one or more images, or click to browse.</p>
  `;

  if (!artifact.images.length) {
    elements.imageGallery.innerHTML =
      '<div class="empty-state">No images attached yet. Drop files here to start a gallery.</div>';
  } else {
    elements.imageGallery.innerHTML = artifact.images
      .map(image => renderImageCard(image))
      .join('');
  }

  startImagePollingIfNeeded();
}

function renderImageCard(image) {
  if (image.status === 'processed' && image.url) {
    return `
      <article class="image-card">
        <div class="image-frame">
          <img src="${escapeAttribute(image.url)}" alt="${escapeAttribute(image.originalFilename)}" />
        </div>
        <div class="image-meta">
          <span class="image-name">${escapeHtml(image.originalFilename)}</span>
          <span class="image-state">Ready${image.width && image.height ? ` · ${image.width}×${image.height}` : ''}</span>
        </div>
      </article>
    `;
  }

  if (image.status === 'failed') {
    return `
      <article class="image-card pending">
        <div class="image-frame loading-frame failed-frame">
          <div class="image-spinner failed-spinner"></div>
          <span class="loading-label">Processing failed</span>
        </div>
        <div class="image-meta">
          <span class="image-name">${escapeHtml(image.originalFilename)}</span>
          <span class="image-state">${escapeHtml(image.errorMessage || 'Please try uploading this image again.')}</span>
        </div>
      </article>
    `;
  }

  return `
    <article class="image-card pending">
      <div class="image-frame loading-frame">
        <div class="image-spinner"></div>
        <span class="loading-label">Optimizing image…</span>
      </div>
      <div class="image-meta">
        <span class="image-name">${escapeHtml(image.originalFilename)}</span>
        <span class="image-state">${image.status === 'processing' ? 'Normalizing to WebP' : 'Queued for worker'}</span>
      </div>
    </article>
  `;
}

async function refreshArtifacts(selectedId) {
  state.artifacts = await api('/api/artifacts');
  renderArtifactList();

  if (selectedId) {
    await loadArtifact(selectedId, false);
  }
}

async function loadArtifact(id, refreshList = true) {
  state.currentArtifact = await api(`/api/artifacts/${id}`);
  if (refreshList) {
    await refreshArtifacts(id);
    return;
  }

  renderArtifactList();
  renderForm();
}

async function createArtifact(parentId = null) {
  setStatus('Creating...');
  const artifact = await api('/api/artifacts', {
    method: 'POST',
    body: JSON.stringify(parentId ? { parentId } : {}),
  });

  await refreshArtifacts(artifact.id);
  setStatus('Ready');
}

function scheduleSave() {
  if (state.isLoadingForm || !state.currentArtifact) {
    return;
  }

  clearTimeout(state.saveTimer);
  setStatus('Saving soon...');
  state.saveTimer = setTimeout(() => {
    void saveCurrentArtifact();
  }, 350);
}

async function saveCurrentArtifact() {
  if (!state.currentArtifact) {
    return;
  }

  const payload = {
    name: elements.name.value,
    desc: elements.desc.value,
  };

  const latValue = elements.lat.value.trim();
  const lngValue = elements.lng.value.trim();

  if ((latValue && !lngValue) || (!latValue && lngValue)) {
    setStatus('Latitude and longitude must be filled together.');
    return;
  }

  if (latValue && lngValue) {
    payload.lat = Number(latValue);
    payload.lng = Number(lngValue);
  } else if (state.currentArtifact.loc) {
    payload.clearLocation = true;
  }

  setStatus('Saving...');

  try {
    state.currentArtifact = await api(`/api/artifacts/${state.currentArtifact.id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    });
    await refreshArtifacts(state.currentArtifact.id);
    setStatus('Saved');
  } catch (error) {
    setStatus(error.message);
  }
}

async function uploadImages(fileList) {
  if (!state.currentArtifact) {
    return;
  }

  const files = [...fileList].filter(file => file.type.startsWith('image/'));
  if (!files.length) {
    setStatus('Please drop image files.');
    return;
  }

  const artifactId = state.currentArtifact.id;
  const formData = new FormData();
  for (const file of files) {
    formData.append('images', file);
  }

  setStatus(`Uploading ${files.length} image${files.length === 1 ? '' : 's'}...`);

  try {
    const response = await api(`/api/artifacts/${artifactId}/images`, {
      method: 'POST',
      body: formData,
    });

    if (state.currentArtifact?.id === artifactId) {
      state.currentArtifact.images = [
        ...(state.currentArtifact.images || []),
        ...response.images,
      ];
      renderImageSection();
      startImagePollingIfNeeded();
    }

    setStatus('Images queued for processing');
  } catch (error) {
    setStatus(error.message);
  }
}

async function syncArtifactImages() {
  if (!state.currentArtifact) {
    stopImagePolling();
    return;
  }

  try {
    const response = await api(`/api/artifacts/${state.currentArtifact.id}/images`);
    if (!state.currentArtifact) {
      return;
    }

    state.currentArtifact.images = response.images;
    renderImageSection();
  } catch (error) {
    stopImagePolling();
    setStatus(error.message);
  }
}

function startImagePollingIfNeeded() {
  if (!hasPendingImages()) {
    stopImagePolling();
    return;
  }

  if (state.imagePollTimer) {
    return;
  }

  state.imagePollTimer = window.setInterval(() => {
    void syncArtifactImages();
  }, 2000);
}

function stopImagePolling() {
  if (state.imagePollTimer) {
    window.clearInterval(state.imagePollTimer);
    state.imagePollTimer = null;
  }
}

function hasPendingImages() {
  return Boolean(
    state.currentArtifact?.images?.some(
      image => image.status === 'queued' || image.status === 'processing',
    ),
  );
}

function setDragState(isDragging) {
  state.isDraggingImages = isDragging;
  renderImageSection();
}

function escapeHtml(value) {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

function escapeAttribute(value) {
  return escapeHtml(value);
}

elements.newRootButton.addEventListener('click', () => {
  void createArtifact();
});

elements.refreshButton.addEventListener('click', () => {
  void refreshArtifacts(state.currentArtifact?.id);
});

elements.newChildButton.addEventListener('click', () => {
  if (!state.currentArtifact) {
    return;
  }

  void createArtifact(state.currentArtifact.id);
});

elements.deleteArtifactButton.addEventListener('click', () => {
  if (!state.currentArtifact) {
    return;
  }

  const artifact = state.currentArtifact;
  const confirmed = window.confirm(
    `Delete "${displayName(artifact)}" and all nested artifacts?`,
  );

  if (!confirmed) {
    return;
  }

  void deleteArtifact(artifact.id);
});

for (const control of [elements.name, elements.desc, elements.lat, elements.lng]) {
  control.addEventListener('input', scheduleSave);
}

elements.imageDropzone.addEventListener('click', () => {
  if (!state.currentArtifact) {
    return;
  }

  elements.imageInput.click();
});

elements.imageInput.addEventListener('change', event => {
  const files = event.target.files;
  if (files?.length) {
    void uploadImages(files);
  }

  event.target.value = '';
});

for (const eventName of ['dragenter', 'dragover']) {
  elements.imageDropzone.addEventListener(eventName, event => {
    event.preventDefault();
    if (state.currentArtifact) {
      setDragState(true);
    }
  });
}

for (const eventName of ['dragleave', 'dragend']) {
  elements.imageDropzone.addEventListener(eventName, event => {
    event.preventDefault();
    setDragState(false);
  });
}

elements.imageDropzone.addEventListener('drop', event => {
  event.preventDefault();
  setDragState(false);

  if (!state.currentArtifact) {
    return;
  }

  const files = event.dataTransfer?.files;
  if (files?.length) {
    void uploadImages(files);
  }
});

void refreshArtifacts().then(() => {
  renderForm();
  setStatus('Ready');
});

async function deleteArtifact(id) {
  setStatus('Deleting...');

  try {
    await api(`/api/artifacts/${id}`, {
      method: 'DELETE',
    });
    state.currentArtifact = null;
    stopImagePolling();
    await refreshArtifacts();
    renderForm();
    setStatus('Deleted');
  } catch (error) {
    setStatus(error.message);
  }
}
