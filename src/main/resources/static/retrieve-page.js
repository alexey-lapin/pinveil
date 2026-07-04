import { decryptPayload } from "./crypto.js";
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
            const decryptedEnvelope = await decryptPayload(blobBytes, passphrase, pin);
            renderDecryptedEnvelope(decryptedEnvelope);
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

function renderDecryptedEnvelope(envelope) {
    const textOutput = document.getElementById("text-output");
    const fileOutput = document.getElementById("file-output");

    textOutput.classList.add("hidden");
    fileOutput.classList.add("hidden");

    if (envelope.metadata.type === "text") {
        const text = new TextDecoder().decode(envelope.payload);
        document.getElementById("message-output").textContent = text;
        document.getElementById("copy-message-button").onclick = async () => {
            await navigator.clipboard.writeText(text);
        };
        textOutput.classList.remove("hidden");
        return;
    }

    const fileBlob = new Blob([envelope.payload], { type: envelope.metadata.mimeType || "application/octet-stream" });
    document.getElementById("file-output-name").textContent = `${envelope.metadata.name} (${formatBytes(fileBlob.size)})`;
    document.getElementById("download-file-button").onclick = () => {
        const url = URL.createObjectURL(fileBlob);
        const anchor = document.createElement("a");
        anchor.href = url;
        anchor.download = envelope.metadata.name || "download.bin";
        anchor.click();
        URL.revokeObjectURL(url);
    };
    fileOutput.classList.remove("hidden");
}
