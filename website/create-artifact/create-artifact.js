const template = document.createElement("template");

const templates = [
    `
    <link rel="stylesheet" href="../website/create-artifact/create-artifact.css">
    <div class="create-artifact-container">
        <div class="create-artifact-top-bar">
            <div class="create-artifact-title">Create Artifact</div>
            <button class="create-artifact-exit-button" id="create-artifact-exit-button">X</button>
        </div>
        <div class="create-artifact-media-upload">
            <div class="create-artifact-media-upload-placeholder-image">will be little placeholder image</div>
            <input class="create-artifact-media-upload-input" type="file" accept="image/*,video/*" multiple></input>
        </div>
        <div class="create-artifact-data-fields">
            <input class="create-artifact-data-field-name"></input>
            <input class="create-artifact-data-field-description"></input>
            <input class="create-artifact-data-field-tags"></input>
            <input class="create-artifact-data-field-location-latitude" type=""></input>
            <input class="create-artifact-data-field-location-longitude"></input>
        </div>
    </div>
    `,
];

template.innerHTML = templates[0];

class CreateArtifact extends HTMLElement {
    constructor() {
        super();
        this.shadow = this.attachShadow({mode: "open"});
        this.shadow.appendChild(template.content.cloneNode(true));

        // Initialize refs to Null
        this.createArtifactExitButton = null;
    }

    setupDomReferences() {
        // Set up references to buttons
        this.createArtifactExitButton = this.shadow.getElementById("create-artifact-exit-button");
    }

    attachListeners() {
        // attaches all event listeners
        if(this.createArtifactExitButton) {
            this.createArtifactExitButton.addEventListener('click', () => this.createArtifactExitEvent());
        }
    }

    connectedCallback() {
        // Sets up dashboard for use
        this.setupDomReferences();
        this.attachListeners();
    }

    disconnectedCallback(){
        // removes all event listeners
        if(this.createArtifactExitButton) {
            this.createArtifactExitButton.removeEventListener("click", this.createArtifactExitEvent);
        }
    }
    
    createArtifactExitEvent() {
        console.log("Hello");
    }
}

customElements.define("create-artifact-component", CreateArtifact);
