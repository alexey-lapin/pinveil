import { encodeEnvelope, encryptPayload, decryptPayload, generatePassphrase } from "./crypto.js";

const config = JSON.parse(document.getElementById("app-config")?.textContent ?? "{}");

document.addEventListener("DOMContentLoaded", () => {
    const page = document.body.dataset.page;
    if (page === "create") {
        initCreatePage();
    }
    if (page === "retrieve") {
        initRetrievePage();
    }
});

function initCreatePage() {
    const form = document.getElementById("create-form");
    const ttlPreset = document.getElementById("ttlPreset");
    const customTtlField = document.getElementById("custom-ttl-field");
    const status = document.getElementById("create-status");
    const submitButton = document.getElementById("submit-button");
    const resultReady = document.getElementById("result-ready");
    const createInputs = document.getElementById("create-inputs");
    const copyUrlButton = document.getElementById("copy-url-button");
    const modeButtons = Array.from(document.querySelectorAll(".mode-chip"));

    populateTtlOptions(ttlPreset);
    bindModeSwitcher(modeButtons);

    ttlPreset.addEventListener("change", () => {
        customTtlField.classList.toggle("hidden", ttlPreset.value !== "custom");
    });

    copyUrlButton.addEventListener("click", async () => {
        const shareUrl = document.getElementById("share-url").value;
        await navigator.clipboard.writeText(shareUrl);
        setFeedback(status, "Link copied to clipboard.", "success");
    });

    form.addEventListener("submit", async (event) => {
        event.preventDefault();
        submitButton.disabled = true;
        setFeedback(status, "Encrypting locally and uploading ciphertext...", "success");

        try {
            const payloadType = document.getElementById("payloadType").value;
            const pin = document.getElementById("pin").value.trim();
            const ttlSeconds = resolveTtlSeconds(ttlPreset, document.getElementById("customTtlMinutes"));
            validatePin(pin);

            const words = await loadWordList();
            const passphrase = generatePassphrase(words);
            const payload = await buildPayload(payloadType);

            if (payload.byteLength > config.maxPayloadBytes) {
                throw new Error("Payload exceeds the 25 MB limit before encryption.");
            }

            const blob = await encryptPayload(payload, passphrase, pin, config.pbkdf2Iterations);
            const formData = new FormData();
            formData.append("blob", new Blob([blob]), "blob.bin");
            formData.append("pin", pin);
            formData.append("ttl", String(ttlSeconds));

            const response = await fetch("/api/messages", {
                method: "POST",
                body: formData,
                cache: "no-store"
            });

            const responseBody = await parseJson(response);
            if (!response.ok) {
                throw new Error(responseBody.error ?? "Failed to create secure link.");
            }

            const shareUrl = `${window.location.origin}/message/${responseBody.id}#${passphrase}`;
            document.getElementById("share-url").value = shareUrl;
            document.getElementById("expiry-label").textContent = `Expires at ${new Date(responseBody.expiresAt).toLocaleString()}`;
            createInputs.classList.add("hidden");
            resultReady.classList.remove("hidden");
            setFeedback(status, "Secure link generated. Share the URL and PIN separately.", "success");
        } catch (error) {
            setFeedback(status, error.message || "Failed to create secure link.", "error");
        } finally {
            submitButton.disabled = false;
        }
    });
}

function initRetrievePage() {
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
                method: "POST",
                headers: {
                    "Content-Type": "application/json"
                },
                body: JSON.stringify({ pin }),
                cache: "no-store"
            });

            if (!response.ok) {
                const errorBody = await parseJson(response);
                throw new Error(errorBody.error ?? "Message unavailable.");
            }

            const blobBytes = new Uint8Array(await response.arrayBuffer());
            const decryptedEnvelope = await decryptPayload(blobBytes, passphrase, pin);
            renderDecryptedEnvelope(decryptedEnvelope, outputCard);
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

function populateTtlOptions(select) {
    for (const preset of config.ttlPresets ?? []) {
        const option = document.createElement("option");
        option.value = String(preset.seconds);
        option.textContent = preset.label;
        if (preset.seconds === config.defaultTtlSeconds) {
            option.selected = true;
        }
        select.append(option);
    }
    const customOption = document.createElement("option");
    customOption.value = "custom";
    customOption.textContent = "Custom";
    select.append(customOption);
}

function bindModeSwitcher(modeButtons) {
    const payloadType = document.getElementById("payloadType");
    const textField = document.getElementById("text-field");
    const fileField = document.getElementById("file-field");

    for (const button of modeButtons) {
        button.addEventListener("click", () => {
            for (const chip of modeButtons) {
                chip.classList.toggle("is-active", chip === button);
            }
            const mode = button.dataset.mode;
            payloadType.value = mode;
            textField.classList.toggle("hidden", mode !== "text");
            fileField.classList.toggle("hidden", mode !== "file");
        });
    }
}

async function loadWordList() {
    if (!window.__pinveilWordListPromise) {
        window.__pinveilWordListPromise = fetch("/static/eff_large_wordlist.txt", { cache: "force-cache" })
            .then((response) => response.text())
            .then((text) => text.trim().split(/\n+/).map((line) => line.trim().split(/\s+/, 2)[1]));
    }
    return window.__pinveilWordListPromise;
}

function resolveTtlSeconds(ttlPreset, customTtlInput) {
    if (ttlPreset.value !== "custom") {
        return Number(ttlPreset.value);
    }
    const minutes = Number(customTtlInput.value);
    if (!Number.isInteger(minutes) || minutes < 1 || minutes > 1440) {
        throw new Error("Custom expiry must be between 1 minute and 24 hours.");
    }
    return minutes * 60;
}

function validatePin(pin) {
    if (!/^\d{6}$/.test(pin)) {
        throw new Error("PIN must be exactly six digits.");
    }
}

async function buildPayload(payloadType) {
    if (payloadType === "text") {
        const text = document.getElementById("messageText").value;
        if (!text) {
            throw new Error("Enter some text before generating a link.");
        }
        return encodeEnvelope({ type: "text" }, new TextEncoder().encode(text));
    }

    const file = document.getElementById("messageFile").files[0];
    if (!file) {
        throw new Error("Choose a file before generating a link.");
    }
    if (file.size > config.maxPayloadBytes) {
        throw new Error("The selected file exceeds the 25 MB limit.");
    }

    const fileBytes = new Uint8Array(await file.arrayBuffer());
    return encodeEnvelope(
        {
            type: "file",
            name: file.name,
            mimeType: file.type || "application/octet-stream"
        },
        fileBytes
    );
}

function renderDecryptedEnvelope(envelope, outputCard) {
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

function setFeedback(element, message, kind) {
    element.textContent = message;
    element.classList.remove("error", "success");
    if (kind) {
        element.classList.add(kind);
    }
}

async function parseJson(response) {
    const contentType = response.headers.get("content-type") || "";
    if (!contentType.includes("application/json")) {
        return {};
    }
    return response.json();
}

function formatBytes(bytes) {
    if (bytes < 1024) {
        return `${bytes} B`;
    }
    if (bytes < 1024 * 1024) {
        return `${(bytes / 1024).toFixed(1)} KB`;
    }
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
