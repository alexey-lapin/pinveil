@param String messageId
@param String clientConfigJson
<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Pinveil - Open secure message</title>
    <link rel="stylesheet" href="/static/app.css">
</head>
<body data-page="retrieve" data-message-id="${messageId}">
<script id="app-config" type="application/json">$unsafe{clientConfigJson}</script>
<main class="shell shell-retrieve">
    <section class="card retrieve-card">
        <p class="eyebrow">Message</p>

        <form id="retrieve-form" class="retrieve-form">
            <label class="field">
                <span>PIN</span>
                <input id="retrieve-pin" inputmode="numeric" autocomplete="one-time-code" pattern="[0-9]{6}" maxlength="6" placeholder="123456" required>
            </label>

            <div class="status-row">
                <button class="primary-action" id="retrieve-button" type="submit">Open message</button>
            </div>

            <div id="retrieve-status" class="feedback" aria-live="polite"></div>
        </form>
    </section>

    <section class="card output-card hidden" id="output-card">
        <div id="text-output" class="hidden">
            <div class="result-header">
                <p class="eyebrow">Decrypted text</p>
                <h2>Message contents</h2>
            </div>
            <pre id="message-output"></pre>
            <div class="action-row">
                <button class="secondary-action" type="button" id="copy-message-button">Copy text</button>
                <span class="inline-note">This content never appeared on the network in plaintext.</span>
            </div>
        </div>

        <div id="file-output" class="hidden">
            <div class="result-header">
                <p class="eyebrow">Decrypted file</p>
                <h2>Download is ready</h2>
            </div>
            <p id="file-output-name"></p>
            <div class="action-row">
                <button class="secondary-action" type="button" id="download-file-button">Download file</button>
                <span class="inline-note">File name and MIME type were stored inside the encrypted payload.</span>
            </div>
        </div>
    </section>
    <noscript><p class="card feedback visible">This page needs JavaScript for local cryptography.</p></noscript>
</main>
<script type="module" src="/static/app.js"></script>
</body>
</html>
