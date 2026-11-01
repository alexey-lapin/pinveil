import { initCreatePage } from "./create-page.js";
import { initRetrievePage } from "./retrieve-page.js";

document.addEventListener("DOMContentLoaded", () => {
    const page = document.body.dataset.page;
    if (page === "create") {
        initCreatePage();
    }
    if (page === "retrieve") {
        initRetrievePage();
    }
});
