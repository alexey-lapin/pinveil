/** Shared helpers for status feedback, JSON parsing, byte formatting, copying, and downloads. */

export function setFeedback(element, message, kind) {
    element.textContent = message;
    element.classList.remove("error", "success");
    if (kind) {
        element.classList.add(kind);
    }
}

export async function parseJson(response) {
    const contentType = response.headers.get("content-type") || "";
    if (!contentType.includes("application/json")) {
        return {};
    }
    try {
        return await response.json();
    } catch {
        return {};
    }
}

export function formatBytes(bytes) {
    if (bytes < 1024) {
        return `${bytes} B`;
    }
    if (bytes < 1024 * 1024) {
        return `${(bytes / 1024).toFixed(1)} KB`;
    }
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * Copies `text`, and says so on the button for a moment. Without clipboard access (plain HTTP
 * off localhost, or a denied permission) it selects `field` so the user can copy by hand.
 */
export async function copyText(text, button, field) {
    const label = button.textContent;
    try {
        await navigator.clipboard.writeText(text);
        button.textContent = "Copied";
    } catch {
        field?.select();
        button.textContent = "Selected: press Ctrl+C or Cmd+C";
    }
    setTimeout(() => {
        button.textContent = label;
    }, 2000);
}

/**
 * Saves `parts` (anything `Blob` accepts) as a file. The type is always
 * application/octet-stream: the stored MIME type is untrusted, and a download must never be
 * rendered.
 */
export function saveFile(parts, name) {
    const url = URL.createObjectURL(new Blob(parts, { type: "application/octet-stream" }));
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = name;
    document.body.append(anchor);
    anchor.click();
    anchor.remove();
    // Revoking right away can cancel the download in some browsers.
    setTimeout(() => URL.revokeObjectURL(url), 60_000);
}
