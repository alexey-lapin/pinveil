// Loaded as a classic script in <head>, before the stylesheet, so a stored theme applies before
// the first paint. No stored choice means System: the CSS follows prefers-color-scheme.
(() => {
    const KEY = "pinveil-theme";
    const root = document.documentElement;

    const read = () => {
        try {
            const value = localStorage.getItem(KEY);
            return value === "light" || value === "dark" ? value : "system";
        } catch {
            return "system";
        }
    };

    const apply = (theme) => {
        if (theme === "system") {
            root.removeAttribute("data-theme");
        } else {
            root.dataset.theme = theme;
        }
    };

    apply(read());

    const ORDER = ["system", "light", "dark"];
    const NAMES = { system: "System", light: "Light", dark: "Dark" };

    document.addEventListener("DOMContentLoaded", () => {
        const button = document.getElementById("theme-button");
        if (!button) {
            return;
        }
        // Without storage the choice lives here until the page is left.
        let current = read();
        const label = () => {
            const next = ORDER[(ORDER.indexOf(current) + 1) % ORDER.length];
            button.setAttribute("aria-label", `Theme: ${NAMES[current]}. Switch to ${NAMES[next]}.`);
            button.title = `Theme: ${NAMES[current]}`;
            return next;
        };
        label();
        button.addEventListener("click", () => {
            current = label();
            apply(current);
            label();
            try {
                if (current === "system") {
                    localStorage.removeItem(KEY);
                } else {
                    localStorage.setItem(KEY, current);
                }
            } catch {
                // Storage is unavailable; the choice still applies to this page.
            }
        });
    });
})();
