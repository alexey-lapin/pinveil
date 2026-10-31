import { LOST_IN_TRANSIT, OPEN_WARNING, openErrorText, retrieveErrorText, storeModeNotice } from "./compose.js";
import { config } from "./config.js";
import { openPayload } from "./crypto.js";
import { exportNames } from "./export-names.js";
import { isValidPin } from "./random.js";
import { copyText, formatBytes, parseJson, saveFile, setFeedback } from "./ui.js";
import { zipParts } from "./zip.js";

const $ = (id) => document.getElementById(id);

export function initRetrievePage() {
    const form = $("retrieve-form");
    const button = $("retrieve-button");
    const status = $("retrieve-status");
    const messageId = document.body.dataset.messageId;
    const passphrase = window.location.hash.slice(1);

    $("open-warning").textContent = OPEN_WARNING;
    $("store-notice").textContent = storeModeNotice(config.storeMode);

    if (!passphrase) {
        // Without the fragment nothing can be decrypted, so don't let the message be consumed.
        setFeedback(status, "This link is incomplete: the part after # is missing. Ask the sender for the whole link.", "error");
        button.disabled = true;
        $("retrieve-pin").disabled = true;
        return;
    }

    form.addEventListener("submit", async (event) => {
        event.preventDefault();
        const pin = $("retrieve-pin").value.trim();
        if (!isValidPin(pin)) {
            setFeedback(status, "The PIN must be exactly six digits.", "error");
            return;
        }
        button.disabled = true;
        setFeedback(status, "Fetching the encrypted message...", "success");
        let response;
        try {
            response = await fetch(`/api/messages/${encodeURIComponent(messageId)}/retrieve`, {
                headers: { "X-Message-Pin": pin },
                cache: "no-store",
            });
        } catch {
            // No response at all: the request may never have reached the server.
            setFeedback(status, "Couldn't reach the server. Check your connection and try again.", "error");
            button.disabled = false;
            return;
        }
        if (!response.ok) {
            const body = await parseJson(response);
            setFeedback(status, retrieveErrorText(response.status, body.error), "error");
            button.disabled = false;
            return;
        }

        // A 200 means the server has deleted the message. Nothing after this point can be retried.
        form.classList.add("hidden");
        let blob;
        try {
            blob = new Uint8Array(await response.arrayBuffer());
        } catch {
            setFeedback(status, LOST_IN_TRANSIT, "error");
            return;
        }
        try {
            setFeedback(status, "Decrypting in this browser...", "success");
            render(await openPayload(blob, passphrase, pin));
            setFeedback(status, "", null);
        } catch (error) {
            setFeedback(status, openErrorText(error), "error");
        }
    });
}

function render(payload) {
    if (payload.text !== null) {
        $("message-output").textContent = payload.text;
        $("copy-text-button").addEventListener("click", (event) => copyText(payload.text, event.currentTarget, $("message-output")));
        $("text-output").classList.remove("hidden");
    }
    if (payload.files.length > 0) {
        const names = exportNames(payload.files.map((file) => file.name));
        $("download-list").replaceChildren(
            ...payload.files.map((file, i) => {
                const item = document.createElement("li");
                // The original name, shown as plain text; downloads use the safe export name.
                const name = document.createElement("span");
                name.className = "file-name";
                name.textContent = file.name;
                name.title = file.mimeType;
                const size = document.createElement("span");
                size.className = "file-size";
                size.textContent = formatBytes(file.data.length);
                const download = document.createElement("button");
                download.type = "button";
                download.className = "secondary-action";
                download.textContent = "Download";
                download.setAttribute("aria-label", `Download ${file.name}`);
                download.dataset.exportName = names[i];
                download.addEventListener("click", () => saveFile([file.data], names[i]));
                item.append(name, size, download);
                return item;
            }),
        );
        if (payload.files.length > 1) {
            const all = $("download-all-button");
            all.classList.remove("hidden");
            all.addEventListener("click", () => {
                try {
                    saveFile(zipParts(payload.files.map((file, i) => ({ name: names[i], data: file.data }))), "pinveil-files.zip");
                } catch (error) {
                    setFeedback($("retrieve-status"), error.message, "error");
                }
            });
        }
        $("file-output").classList.remove("hidden");
    }
    $("output-card").classList.remove("hidden");
}
