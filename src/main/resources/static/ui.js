/** Shared DOM helpers for status feedback, JSON parsing, and byte formatting. */

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
    return response.json();
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
