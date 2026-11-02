import {
    checkMessage,
    createErrorText,
    creationLimits,
    customTtlMinutes,
    mimeTypeOf,
    storeModeNotice,
    textPart,
    ttlChoices,
    ttlFromMinutes,
} from "./compose.js";
import { config } from "./config.js";
import { sealPayload } from "./crypto.js";
import { blobLength } from "./envelope.js";
import { encodePayload } from "./payload.js";
import { generatePassphrase, generatePin, isValidPin } from "./random.js";
import { copyText, formatBytes, parseJson, setFeedback } from "./ui.js";
import { loadWordList } from "./wordlist.js";

const $ = (id) => document.getElementById(id);

export function initCreatePage() {
    const form = $("create-form");
    const status = $("create-status");
    const submitButton = $("submit-button");
    const pinInput = $("pin");
    const ttlPreset = $("ttl-preset");
    /** The picked files, in the order they were added. */
    let files = [];

    $("store-notice").textContent = storeModeNotice(config.storeMode);
    // The limit covers the whole encrypted message, text and overhead included.
    $("file-limits").textContent =
        `Up to ${config.maxFiles} files · ${formatBytes(config.maxPayloadBytes)} max message size`;
    const messageText = $("message-text");
    const fitText = initAutoGrow(messageText);
    pinInput.value = generatePin();
    $("new-pin-button").addEventListener("click", () => {
        pinInput.value = generatePin();
    });
    initTtl(ttlPreset);

    const renderFiles = () => {
        const list = $("file-list");
        list.replaceChildren(
            ...files.map((file, index) => {
                const item = document.createElement("li");
                const name = document.createElement("span");
                name.className = "file-name";
                name.textContent = file.name;
                name.title = file.name;
                const size = document.createElement("span");
                size.className = "file-size";
                size.textContent = formatBytes(file.size);
                const remove = document.createElement("button");
                remove.type = "button";
                remove.className = "file-clear-btn";
                remove.innerHTML = REMOVE_ICON;
                remove.title = "Remove";
                remove.setAttribute("aria-label", `Remove ${file.name}`);
                remove.addEventListener("click", () => {
                    files = files.filter((_, i) => i !== index);
                    renderFiles();
                    // Focus moves to the next file's remove button, else the previous one, else Browse.
                    const buttons = $("file-list").querySelectorAll(".file-clear-btn");
                    (buttons[Math.min(index, buttons.length - 1)] ?? $("browse-button")).focus();
                });
                item.append(name, size, remove);
                return item;
            }),
        );
    };
    const addFiles = (picked) => {
        files = [...files, ...picked];
        renderFiles();
        // An early warning; the same check runs again on submit.
        try {
            checkMessage({ text: $("message-text").value, files }, config);
            setFeedback(status, "", null);
        } catch (error) {
            setFeedback(status, error.message, "error");
        }
    };
    initFileDrop(addFiles);

    $("copy-url-button").addEventListener("click", (event) => copyText($("share-url").value, event.currentTarget, $("share-url")));
    $("copy-pin-button").addEventListener("click", (event) => copyText($("share-pin").value, event.currentTarget, $("share-pin")));
    $("create-another-button").addEventListener("click", () => {
        files = [];
        renderFiles();
        messageText.value = "";
        fitText();
        $("share-url").value = "";
        $("share-pin").value = "";
        pinInput.value = generatePin();
        $("result-ready").classList.add("hidden");
        $("create-inputs").classList.remove("hidden");
        setFeedback(status, "", null);
        messageText.focus();
    });

    form.addEventListener("submit", async (event) => {
        event.preventDefault();
        submitButton.disabled = true;
        try {
            const created = await create({ text: $("message-text").value, files, pin: pinInput.value.trim() }, status);
            $("share-url").value = created.url;
            fitShareLink();
            $("share-pin").value = created.pin;
            $("expiry-label").textContent = `Expires ${new Date(created.expiresAt).toLocaleString()}`;
            $("create-inputs").classList.add("hidden");
            $("result-ready").classList.remove("hidden");
            setFeedback(status, "", null);
            $("result-title").focus();
        } catch (error) {
            setFeedback(status, createErrorText(error, config), "error");
        } finally {
            submitButton.disabled = false;
        }
    });
}

/** Validates, encrypts, and uploads. Returns `{url, pin, expiresAt}`. */
async function create({ text, files, pin }, status) {
    if (!isValidPin(pin)) {
        throw new Error("The PIN must be exactly six digits.");
    }
    const ttlSeconds = selectedTtl();
    // Every limit is checked from metadata before any file is read.
    const expectedSize = checkMessage({ text, files }, config);

    setFeedback(status, "Encrypting in this browser...", "success");
    const words = await loadWordList();
    const passphrase = generatePassphrase(words);
    const data = await Promise.all(files.map(async (file) => new Uint8Array(await file.arrayBuffer())));
    const payload = encodePayload(
        {
            text: textPart(text),
            files: files.map((file, i) => ({ name: file.name, mimeType: mimeTypeOf(file), data: data[i] })),
        },
        creationLimits(config),
    );
    // A file can change on disk after it was picked; never upload more than was checked.
    if (blobLength(payload.length, config.pbkdf2Iterations) !== expectedSize) {
        throw new Error("A file changed after it was added. Remove it and add it again.");
    }
    const blob = await sealPayload(payload, passphrase, pin, config.pbkdf2Iterations);

    setFeedback(status, "Uploading ciphertext...", "success");
    const body = new FormData();
    // A fixed part filename: the original names exist only inside the ciphertext.
    body.append("blob", new Blob([blob], { type: "application/octet-stream" }), "blob");
    body.append("pin", pin);
    body.append("ttl", String(ttlSeconds));
    const response = await fetch("/api/messages", { method: "POST", body, cache: "no-store" });
    const result = await parseJson(response);
    if (!response.ok) {
        throw new Error(
            response.status === 429
                ? "Too many messages from your address. Wait a minute and try again."
                : (result.error ?? "The server refused the message."),
        );
    }
    return {
        url: `${window.location.origin}/message/${encodeURIComponent(result.id)}#${passphrase}`,
        pin,
        expiresAt: result.expiresAt,
    };
}

function initTtl(select) {
    for (const choice of ttlChoices(config)) {
        const option = document.createElement("option");
        option.value = String(choice.seconds);
        option.textContent = choice.label;
        option.selected = choice.seconds === config.defaultTtlSeconds;
        select.append(option);
    }
    const range = customTtlMinutes(config);
    const field = $("custom-ttl-field");
    if (range !== null) {
        const custom = document.createElement("option");
        custom.value = "custom";
        custom.textContent = "Custom";
        select.append(custom);
        const input = $("custom-ttl-minutes");
        input.min = String(range.min);
        input.max = String(range.max);
        input.value = String(Math.min(Math.max(Math.round(config.defaultTtlSeconds / 60), range.min), range.max));
        $("custom-ttl-range").textContent = `From ${range.min} to ${range.max} minutes.`;
    }
    const showCustom = () => field.classList.toggle("hidden", select.value !== "custom");
    select.addEventListener("change", showCustom);
    showCustom();
}

function selectedTtl() {
    const select = $("ttl-preset");
    return select.value === "custom" ? ttlFromMinutes($("custom-ttl-minutes").value, config) : Number(select.value);
}

const REMOVE_ICON =
    '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" aria-hidden="true"><path d="M18 6 6 18M6 6l12 12"/></svg>';

/** Sizes the link box to the whole link where `field-sizing: content` is missing. */
function fitShareLink() {
    const box = $("share-url");
    if (!CSS.supports("field-sizing", "content")) {
        box.style.height = "auto";
        box.style.height = `${box.scrollHeight + 2}px`;
    }
}

/**
 * Grows the textarea with its content, up to the max-height in the CSS, then it scrolls.
 * Browsers with `field-sizing: content` do this in CSS alone. Returns a refit function.
 */
function initAutoGrow(textarea) {
    if (CSS.supports("field-sizing", "content")) {
        return () => {};
    }
    const fit = () => {
        textarea.style.height = "auto";
        textarea.style.height = `${textarea.scrollHeight + 2}px`;
    };
    textarea.addEventListener("input", fit);
    window.addEventListener("resize", fit);
    fit();
    return fit;
}

function initFileDrop(addFiles) {
    const input = $("message-files");
    const zone = $("file-drop");
    $("browse-button").addEventListener("click", () => input.click());
    input.addEventListener("change", () => {
        addFiles([...input.files]);
        // Clearing lets the same file be picked again after a removal.
        input.value = "";
    });
    // The zone is the whole form: only file drags are ours, so text still drops into the textarea.
    const carriesFiles = (event) => [...event.dataTransfer.types].includes("Files");
    zone.addEventListener("dragover", (event) => {
        if (!carriesFiles(event)) {
            return;
        }
        event.preventDefault();
        zone.classList.add("drag-over");
    });
    zone.addEventListener("dragleave", (event) => {
        if (!zone.contains(event.relatedTarget)) {
            zone.classList.remove("drag-over");
        }
    });
    zone.addEventListener("drop", (event) => {
        if (!carriesFiles(event)) {
            return;
        }
        event.preventDefault();
        zone.classList.remove("drag-over");
        addFiles([...event.dataTransfer.files]);
    });
}
