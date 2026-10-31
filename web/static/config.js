/** The client configuration the server embeds in each page (APP.md, "Client config"). */
export const config = JSON.parse(document.getElementById("app-config")?.textContent ?? "{}");
