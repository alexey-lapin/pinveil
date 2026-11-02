import { initCreatePage } from "./create-page.js";
import { initRetrievePage } from "./retrieve-page.js";

document.addEventListener("DOMContentLoaded", () => {
    // "How it works" opens a dialog; closing it returns focus to the button.
    for (const button of document.querySelectorAll("[data-open-dialog]")) {
        const dialog = document.getElementById(button.dataset.openDialog);
        button.addEventListener("click", () => dialog.showModal());
        dialog.addEventListener("click", (event) => {
            if (event.target === dialog) {
                dialog.close();
            }
        });
    }
    const page = document.body.dataset.page;
    if (page === "create") {
        initCreatePage();
    }
    if (page === "retrieve") {
        initRetrievePage();
    }
});
