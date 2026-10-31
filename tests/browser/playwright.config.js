import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
    testDir: ".",
    testMatch: "*.spec.js",
    // PBKDF2 at 600,000 iterations takes a while in some browsers, and CI machines are slow.
    timeout: 60_000,
    expect: { timeout: 20_000 },
    fullyParallel: true,
    forbidOnly: Boolean(process.env.CI),
    retries: 0,
    reporter: process.env.CI ? [["list"], ["github"]] : "list",
    use: { acceptDownloads: true },
    projects: [
        { name: "chromium", use: { ...devices["Desktop Chrome"] } },
        { name: "firefox", use: { ...devices["Desktop Firefox"] } },
        { name: "webkit", use: { ...devices["Desktop Safari"] } },
    ],
});
