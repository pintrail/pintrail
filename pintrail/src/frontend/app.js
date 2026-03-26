const state = {
  artifacts: [],
  currentArtifact: null,
  saveTimer: null,
  isLoadingForm: false,
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
};

async function api(path, options = {}) {
  const response = await fetch(path, {
    headers: {
      'Content-Type': 'application/json',
    },
    ...options,
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
  state.isLoadingForm = false;
  renderChildren();
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

function escapeHtml(value) {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
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
    await refreshArtifacts();
    renderForm();
    setStatus('Deleted');
  } catch (error) {
    setStatus(error.message);
  }
}
