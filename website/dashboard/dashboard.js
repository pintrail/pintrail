const template = document.createElement("template");

const templates = [
    `
    <link rel="stylesheet" href="../website/dashboard/dashboard.css">
    <div class="dashboard-container">
        <button class="add-artifact-button" id="add-artifact-button">add artifact</button>
    </div>
    `,
];

template.innerHTML = templates[0];

class Dashboard extends HTMLElement {
    constructor() {
        super();
        this.shadow = this.attachShadow({mode: "open"});
        this.shadow.appendChild(template.content.cloneNode(true));

        // Initialize refs to Null
        this.addArtifactButton = null;
    }

    setupDomReferences() {
        // Set up references to buttons
        this.addArtifactButton = this.shadow.getElementById("add-artifact-button");
    }

    attachListeners() {
        // attaches all event listeners
        if(this.addArtifactButton) {
            this.addArtifactButton.addEventListener('click', () => this.addArtifactEvent());
        }
    }

    connectedCallback() {
        // Sets up dashboard for use
        this.setupDomReferences();
        this.attachListeners();
    }

    disconnectedCallback(){
        // removes all event listeners
        if(this.addArtifactButton) {
            this.addArtifactButton.removeEventListener("click", this.addArtifactEvent)
        }
    }
    
    addArtifactEvent() {
        console.log("Hello")
    }
}

customElements.define("dashboard-component", Dashboard);
