import { openPayload } from "./crypto.js";
import { validatePin } from "./config.js";
import { setFeedback, parseJson, formatBytes } from "./ui.js";

export function initRetrievePage() {
    const retrieveForm = document.getElementById("retrieve-form");
    const retrieveButton = document.getElementById("retrieve-button");
    const status = document.getElementById("retrieve-status");
    const outputCard = document.getElementById("output-card");
    const messageId = document.body.dataset.messageId;
    const passphrase = window.location.hash.slice(1);

    if (!passphrase) {
        setFeedback(status, "This link is missing its passphrase fragment.", "error");
    }

    retrieveForm.addEventListener("submit", async (event) => {
        event.preventDefault();
        retrieveButton.disabled = true;
        setFeedback(status, "Requesting ciphertext from the server...", "success");

        try {
            if (!passphrase) {
                throw new Error("Missing passphrase fragment.");
            }
            const pin = document.getElementById("retrieve-pin").value.trim();
            validatePin(pin);

            const response = await fetch(`/api/messages/${encodeURIComponent(messageId)}/retrieve`, {
                method: "GET",
                headers: {
                    "X-Message-Pin": pin
                },
                cache: "no-store"
            });

            if (!response.ok) {
                const errorBody = await parseJson(response);
                throw new Error(errorBody.error ?? "Message unavailable.");
            }

            const blobBytes = new Uint8Array(await response.arrayBuffer());
            renderPayload(await openPayload(blobBytes, passphrase, pin));
            retrieveForm.classList.add("hidden");
            outputCard.classList.remove("hidden");
            setFeedback(status, "Decryption complete.", "success");
        } catch (error) {
            setFeedback(status, error.message || "Unable to open message.", "error");
        } finally {
            retrieveButton.disabled = false;
        }
    });
}

// Shows the text or the first file. Phase 3 replaces this with the multi-file view.
function renderPayload(payload) {
    const textOutput = document.getElementById("text-output");
    const fileOutput = document.getElementById("file-output");

    textOutput.classList.add("hidden");
    fileOutput.classList.add("hidden");

    if (payload.text !== null) {
        const text = payload.text;
        document.getElementById("message-output").textContent = text;
        document.getElementById("copy-message-button").onclick = async () => {
            await navigator.clipboard.writeText(text);
        };
        textOutput.classList.remove("hidden");
        return;
    }

    const [file] = payload.files;
    const fileBlob = new Blob([file.data], { type: file.mimeType });
    document.getElementById("file-output-name").textContent = `${file.name} (${formatBytes(fileBlob.size)})`;
    document.getElementById("download-file-button").onclick = () => {
        const url = URL.createObjectURL(fileBlob);
        const anchor = document.createElement("a");
        anchor.href = url;
        anchor.download = file.name;
        anchor.click();
        URL.revokeObjectURL(url);
    };
    fileOutput.classList.remove("hidden");
}
