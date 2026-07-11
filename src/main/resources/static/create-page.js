import { encodeEnvelope, encryptPayload, generatePassphrase } from "./crypto.js";
import { config, validatePin, maxPayloadMegabytes, MAX_CUSTOM_TTL_MINUTES } from "./config.js";
import { setFeedback, parseJson } from "./ui.js";
import { loadWordList } from "./wordlist.js";

export function initCreatePage() {
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
    initFileDrop();

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
                throw new Error(`Payload exceeds the ${maxPayloadMegabytes()} MB limit before encryption.`);
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

function initFileDrop() {
    const fileInput = document.getElementById("messageFile");
    const dropZone = document.getElementById("file-drop");
    const idleView = document.getElementById("file-drop-idle");
    const chosenView = document.getElementById("file-drop-chosen");
    const chosenName = document.getElementById("chosen-file-name");
    const clearBtn = document.getElementById("file-clear-btn");

    function showFile(name) {
        chosenName.textContent = name;
        idleView.classList.add("hidden");
        chosenView.classList.remove("hidden");
    }

    function clearFile() {
        fileInput.value = "";
        idleView.classList.remove("hidden");
        chosenView.classList.add("hidden");
    }

    fileInput.addEventListener("change", () => {
        if (fileInput.files[0]) showFile(fileInput.files[0].name);
    });

    clearBtn.addEventListener("click", (e) => {
        e.preventDefault();
        clearFile();
    });

    dropZone.addEventListener("dragover", (e) => {
        e.preventDefault();
        dropZone.classList.add("drag-over");
    });

    dropZone.addEventListener("dragleave", (e) => {
        if (!dropZone.contains(e.relatedTarget)) {
            dropZone.classList.remove("drag-over");
        }
    });

    dropZone.addEventListener("drop", (e) => {
        e.preventDefault();
        dropZone.classList.remove("drag-over");
        if (e.dataTransfer.files[0]) {
            fileInput.files = e.dataTransfer.files;
            showFile(e.dataTransfer.files[0].name);
        }
    });
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

function resolveTtlSeconds(ttlPreset, customTtlInput) {
    if (ttlPreset.value !== "custom") {
        return Number(ttlPreset.value);
    }
    const minutes = Number(customTtlInput.value);
    if (!Number.isInteger(minutes) || minutes < 1 || minutes > MAX_CUSTOM_TTL_MINUTES) {
        throw new Error("Custom expiry must be between 1 minute and 24 hours.");
    }
    return minutes * 60;
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
        throw new Error(`The selected file exceeds the ${maxPayloadMegabytes()} MB limit.`);
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
