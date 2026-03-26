const state = {
  authUser: null,
  users: [],
  artifacts: [],
  currentArtifact: null,
  saveTimer: null,
  imagePollTimer: null,
  isLoadingForm: false,
  isDraggingImages: false,
  imageModal: {
    isOpen: false,
    isFullscreen: false,
    image: null,
  },
};

const elements = {
  authScreen: document.getElementById('auth-screen'),
  portalShell: document.getElementById('portal-shell'),
  roleBanner: document.getElementById('role-banner'),
  loginForm: document.getElementById('login-form'),
  loginEmail: document.getElementById('login-email'),
  loginPassword: document.getElementById('login-password'),
  loginStatus: document.getElementById('login-status'),
  logoutButton: document.getElementById('logout-button'),
  adminPanel: document.getElementById('admin-panel'),
  userCreateForm: document.getElementById('user-create-form'),
  userEmail: document.getElementById('user-email'),
  userPassword: document.getElementById('user-password'),
  userRole: document.getElementById('user-role'),
  userStatus: document.getElementById('user-status'),
  userList: document.getElementById('user-list'),
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
  imageModal: document.getElementById('image-modal'),
  imageModalBackdrop: document.getElementById('image-modal-backdrop'),
  imageModalDialog: document.getElementById('image-modal-dialog'),
  imageModalPreview: document.getElementById('image-modal-preview'),
  imageModalTitle: document.getElementById('image-modal-title'),
  imageModalDelete: document.getElementById('image-modal-delete'),
  imageModalExpand: document.getElementById('image-modal-expand'),
  imageModalClose: document.getElementById('image-modal-close'),
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
    if (response.status === 401) {
      handleSignedOutState();
    }
    throw new Error(payload.message || 'Request failed.');
  }

  return response.json();
}

function setStatus(message) {
  elements.saveStatus.textContent = message;
}

function setLoginStatus(message) {
  elements.loginStatus.textContent = message;
}

function setUserStatus(message) {
  elements.userStatus.textContent = message;
}

function displayName(artifact) {
  return artifact.name.trim() || 'Untitled Artifact';
}

function isEditor() {
  return state.authUser?.role === 'admin' || state.authUser?.role === 'editor';
}

function isAdmin() {
  return state.authUser?.role === 'admin';
}

function renderAuthState() {
  const isAuthenticated = Boolean(state.authUser);

  elements.authScreen.classList.toggle('hidden', isAuthenticated);
  elements.portalShell.classList.toggle('hidden', !isAuthenticated);
  elements.roleBanner.classList.toggle('hidden', !isAuthenticated || isEditor());
  elements.adminPanel.classList.toggle('hidden', !isAdmin());

  if (state.authUser && !isEditor()) {
    elements.roleBanner.textContent =
      'Signed in with read-only access. You can browse artifacts and images, but editing is disabled.';
  }

  const editingDisabled = !isEditor();
  elements.newRootButton.disabled = editingDisabled;
  elements.newChildButton.disabled = editingDisabled || !state.currentArtifact;
  elements.deleteArtifactButton.disabled = editingDisabled || !state.currentArtifact;

  for (const field of [elements.name, elements.desc, elements.lat, elements.lng]) {
    field.disabled = editingDisabled;
  }

  if (!isEditor()) {
    elements.imageDropzone.classList.add('disabled');
    elements.imageDropzone.setAttribute('aria-disabled', 'true');
  }
}

function handleSignedOutState() {
  state.authUser = null;
  state.users = [];
  state.artifacts = [];
  state.currentArtifact = null;
  stopImagePolling();
  closeImageModal();
  renderAuthState();
  renderUserList();
  renderArtifactList();
  renderForm();
  setLoginStatus('Sign in with an authorized account to continue.');
}

function renderUserList() {
  if (!isAdmin()) {
    elements.userList.innerHTML =
      '<div class="empty-state">Sign in as an admin to manage users.</div>';
    return;
  }

  if (!state.users.length) {
    elements.userList.innerHTML =
      '<div class="empty-state">No users found yet. Create one above.</div>';
    return;
  }

  elements.userList.innerHTML = state.users
    .map(
      user => `
        <article class="user-card">
          <div class="user-card-header">
            <div>
              <div class="user-email">${escapeHtml(user.email)}</div>
              <div class="user-meta">Role: ${escapeHtml(user.role)} · ${user.isActive ? 'Active' : 'Inactive'}</div>
            </div>
            <div class="user-meta">Created ${new Date(user.createdAt).toLocaleString()}</div>
          </div>
          <form class="user-controls" data-user-id="${escapeAttribute(user.id)}">
            <label>
              <span>Email</span>
              <input type="email" name="email" value="${escapeAttribute(user.email)}" />
            </label>
            <label>
              <span>Role</span>
              <select name="role">
                ${renderRoleOptions(user.role)}
              </select>
            </label>
            <label>
              <span>Reset Password</span>
              <input type="password" name="password" placeholder="Leave blank to keep current password" />
            </label>
            <button class="primary-button" type="submit">Save User</button>
            <label class="checkbox-label">
              <input type="checkbox" name="isActive" ${user.isActive ? 'checked' : ''} />
              <span>Active</span>
            </label>
          </form>
        </article>
      `,
    )
    .join('');

  for (const form of elements.userList.querySelectorAll('[data-user-id]')) {
    form.addEventListener('submit', event => {
      event.preventDefault();
      void updateUser(form.dataset.userId, new FormData(form));
    });
  }
}

function renderRoleOptions(selectedRole) {
  return ['viewer', 'editor', 'admin']
    .map(
      role =>
        `<option value="${role}" ${role === selectedRole ? 'selected' : ''}>${role[0].toUpperCase()}${role.slice(1)}</option>`,
    )
    .join('');
}

async function loadUsers() {
  if (!isAdmin()) {
    state.users = [];
    renderUserList();
    return;
  }

  const response = await api('/api/auth/users');
  state.users = response.users;
  renderUserList();
}

async function createUser(event) {
  event.preventDefault();
  if (!isAdmin()) {
    return;
  }

  setUserStatus('Creating user...');

  try {
    const response = await api('/api/auth/users', {
      method: 'POST',
      body: JSON.stringify({
        email: elements.userEmail.value,
        password: elements.userPassword.value,
        role: elements.userRole.value,
      }),
    });

    state.users.push(response.user);
    elements.userCreateForm.reset();
    elements.userRole.value = 'viewer';
    renderUserList();
    setUserStatus(`Created ${response.user.email}.`);
  } catch (error) {
    setUserStatus(error.message);
  }
}

async function updateUser(userId, formData) {
  if (!isAdmin() || !userId) {
    return;
  }

  const payload = {
    email: String(formData.get('email') ?? ''),
    role: String(formData.get('role') ?? 'viewer'),
    isActive: formData.get('isActive') === 'on',
  };
  const password = String(formData.get('password') ?? '').trim();
  if (password) {
    payload.password = password;
  }

  setUserStatus('Updating user...');

  try {
    const response = await api(`/api/auth/users/${userId}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    });

    state.users = state.users.map(user => (user.id === userId ? response.user : user));
    renderUserList();
    setUserStatus(`Updated ${response.user.email}.`);

    if (state.authUser?.id === userId) {
      state.authUser = {
        id: response.user.id,
        email: response.user.email,
        role: response.user.role,
      };
      renderAuthState();
    }
  } catch (error) {
    setUserStatus(error.message);
  }
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
    elements.form.reset();
    elements.artifactId.value = '';
    elements.parentId.value = '';
    renderImageSection();
    state.isLoadingForm = false;
    renderChildren();
    renderAuthState();
    return;
  }

  elements.editorTitle.textContent = displayName(artifact);
  elements.editorSubtitle.textContent = artifact.parentId
    ? `Child artifact linked to parent ${artifact.parentId}.`
    : 'Root artifact with no parent.';
  elements.artifactId.value = artifact.id;
  elements.parentId.value = artifact.parentId ?? '';
  elements.name.value = artifact.name;
  elements.desc.value = artifact.desc;
  elements.lat.value = artifact.loc?.lat ?? '';
  elements.lng.value = artifact.loc?.lng ?? '';
  renderImageSection();
  state.isLoadingForm = false;
  renderChildren();
  renderAuthState();
}

function renderImageSection() {
  const artifact = state.currentArtifact;
  const hasArtifact = Boolean(artifact);
  const canEdit = isEditor();

  elements.imageDropzone.classList.toggle('disabled', !hasArtifact || !canEdit);
  elements.imageDropzone.classList.toggle(
    'dragging',
    state.isDraggingImages && hasArtifact && canEdit,
  );
  elements.imageDropzone.setAttribute(
    'aria-disabled',
    hasArtifact && canEdit ? 'false' : 'true',
  );

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

  elements.imageDropzone.innerHTML = canEdit
    ? `
      <p class="dropzone-title">Drop images here</p>
      <p class="dropzone-copy">Drag and drop one or more images, or click to browse.</p>
    `
    : `
      <p class="dropzone-title">Artifact images</p>
      <p class="dropzone-copy">You have read-only access. Image uploads are disabled.</p>
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
  bindImagePreviewButtons();
}

function renderImageCard(image) {
  if (image.status === 'processed' && image.url) {
    return `
      <article class="image-card">
        <div class="image-frame clickable" data-image-id="${escapeAttribute(image.id)}">
          <img src="${escapeAttribute(image.url)}" alt="${escapeAttribute(image.originalFilename)}" />
        </div>
        <div class="image-meta">
          <span class="image-name">${escapeHtml(image.originalFilename)}</span>
          <span class="image-state">Ready${image.width && image.height ? ` · ${image.width}×${image.height}` : ''}</span>
        </div>
        ${renderDeleteButton(image.id)}
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
        ${renderDeleteButton(image.id)}
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
      ${renderDeleteButton(image.id)}
    </article>
  `;
}

function renderDeleteButton(imageId) {
  if (!isEditor()) {
    return '';
  }

  return `<button class="ghost-button image-delete-button" type="button" data-delete-image-id="${escapeAttribute(imageId)}">Delete Image</button>`;
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
  if (!isEditor()) {
    return;
  }

  setStatus('Creating...');
  const artifact = await api('/api/artifacts', {
    method: 'POST',
    body: JSON.stringify(parentId ? { parentId } : {}),
  });

  await refreshArtifacts(artifact.id);
  setStatus('Ready');
}

function scheduleSave() {
  if (state.isLoadingForm || !state.currentArtifact || !isEditor()) {
    return;
  }

  clearTimeout(state.saveTimer);
  setStatus('Saving soon...');
  state.saveTimer = window.setTimeout(() => {
    void saveCurrentArtifact();
  }, 350);
}

async function saveCurrentArtifact() {
  if (!state.currentArtifact || !isEditor()) {
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
  if (!state.currentArtifact || !isEditor()) {
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

function bindImagePreviewButtons() {
  for (const frame of elements.imageGallery.querySelectorAll('[data-image-id]')) {
    frame.addEventListener('click', () => {
      const image = state.currentArtifact?.images?.find(
        candidate => candidate.id === frame.dataset.imageId,
      );

      if (!image?.url) {
        return;
      }

      openImageModal(image);
    });
  }

  for (const button of elements.imageGallery.querySelectorAll('[data-delete-image-id]')) {
    button.addEventListener('click', event => {
      event.stopPropagation();
      void deleteImage(button.dataset.deleteImageId);
    });
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

function openImageModal(image) {
  state.imageModal.isOpen = true;
  state.imageModal.isFullscreen = false;
  state.imageModal.image = image;
  renderImageModal();
}

function closeImageModal() {
  state.imageModal.isOpen = false;
  state.imageModal.isFullscreen = false;
  state.imageModal.image = null;
  renderImageModal();
}

function toggleImageModalSize() {
  if (!state.imageModal.isOpen) {
    return;
  }

  state.imageModal.isFullscreen = !state.imageModal.isFullscreen;
  renderImageModal();
}

function renderImageModal() {
  const { isOpen, isFullscreen, image } = state.imageModal;

  elements.imageModal.classList.toggle('hidden', !isOpen);
  elements.imageModal.setAttribute('aria-hidden', isOpen ? 'false' : 'true');
  elements.imageModalDialog.classList.toggle('fullscreen', isFullscreen);
  elements.imageModalDelete.disabled = !isOpen || !image || !isEditor();
  elements.imageModalExpand.textContent = isFullscreen ? 'Medium Size' : 'Expand';

  if (!isOpen || !image?.url) {
    elements.imageModalPreview.src = '';
    elements.imageModalPreview.alt = '';
    elements.imageModalTitle.textContent = 'Image preview';
    return;
  }

  elements.imageModalPreview.src = image.url;
  elements.imageModalPreview.alt = image.originalFilename;
  elements.imageModalTitle.textContent = image.originalFilename;
}

async function deleteImage(imageId) {
  if (!state.currentArtifact || !imageId || !isEditor()) {
    return;
  }

  const image = state.currentArtifact.images?.find(candidate => candidate.id === imageId);
  const confirmed = window.confirm(
    `Delete "${image?.originalFilename || 'this image'}" from the artifact?`,
  );

  if (!confirmed) {
    return;
  }

  setStatus('Deleting image...');

  try {
    await api(`/api/artifacts/${state.currentArtifact.id}/images/${imageId}`, {
      method: 'DELETE',
    });

    state.currentArtifact.images = state.currentArtifact.images.filter(
      candidate => candidate.id !== imageId,
    );

    if (state.imageModal.image?.id === imageId) {
      closeImageModal();
    }

    renderImageSection();
    setStatus('Image deleted');
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
  if (!state.currentArtifact || !isEditor()) {
    return;
  }

  void createArtifact(state.currentArtifact.id);
});

elements.deleteArtifactButton.addEventListener('click', () => {
  if (!state.currentArtifact || !isEditor()) {
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
  if (!state.currentArtifact || !isEditor()) {
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
    if (state.currentArtifact && isEditor()) {
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

  if (!state.currentArtifact || !isEditor()) {
    return;
  }

  const files = event.dataTransfer?.files;
  if (files?.length) {
    void uploadImages(files);
  }
});

elements.imageModalClose.addEventListener('click', closeImageModal);
elements.imageModalDelete.addEventListener('click', () => {
  void deleteImage(state.imageModal.image?.id);
});
elements.imageModalExpand.addEventListener('click', toggleImageModalSize);
elements.imageModalBackdrop.addEventListener('click', closeImageModal);

document.addEventListener('keydown', event => {
  if (!state.imageModal.isOpen) {
    return;
  }

  if (event.key === 'Escape') {
    closeImageModal();
  }
});

elements.loginForm.addEventListener('submit', event => {
  event.preventDefault();
  void login();
});

elements.logoutButton.addEventListener('click', () => {
  void logout();
});

elements.userCreateForm.addEventListener('submit', event => {
  void createUser(event);
});

void bootstrap();

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

async function login() {
  setLoginStatus('Signing in...');

  try {
    const response = await api('/api/auth/login', {
      method: 'POST',
      body: JSON.stringify({
        email: elements.loginEmail.value,
        password: elements.loginPassword.value,
      }),
    });

    state.authUser = response.user;
    elements.loginPassword.value = '';
    renderAuthState();
    await Promise.all([refreshArtifacts(), loadUsers()]);
    renderForm();
    setStatus('Ready');
    setLoginStatus('Signed in.');
  } catch (error) {
    setLoginStatus(error.message);
  }
}

async function logout() {
  try {
    await api('/api/auth/logout', {
      method: 'POST',
    });
  } finally {
    handleSignedOutState();
  }
}

async function bootstrap() {
  renderAuthState();
  renderUserList();

  try {
    const response = await api('/api/auth/me');
    state.authUser = response.user;
  } catch (_error) {
    state.authUser = null;
  }

  renderAuthState();

  if (!state.authUser) {
    renderForm();
    return;
  }

  await Promise.all([refreshArtifacts(), loadUsers()]);
  renderForm();
  setStatus('Ready');
}
