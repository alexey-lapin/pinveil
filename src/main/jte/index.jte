@param String clientConfigJson
@param long defaultTtlSeconds
<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Pinveil - Zero-knowledge sharing</title>
    <link rel="stylesheet" href="/static/app.css">
</head>
<body data-page="create">
<script id="app-config" type="application/json">$unsafe{clientConfigJson}</script>
<main class="shell shell-create">
    <section class="create-stage">
        <form id="create-form" class="card form-card create-panel">
            <div id="create-inputs">
                <div class="form-header">
                    <h2>Create secure link</h2>
                    <p>Encrypt text or a file in the browser, then share the generated one-time link and PIN separately.</p>
                </div>

                <div class="mode-switch" role="tablist" aria-label="Payload type">
                    <button type="button" class="mode-chip is-active" data-mode="text">Text</button>
                    <button type="button" class="mode-chip" data-mode="file">File</button>
                </div>

                <input type="hidden" id="payloadType" name="payloadType" value="text">

                <label class="field" id="text-field">
                    <span>Secret text</span>
                    <textarea id="messageText" rows="10" placeholder="Paste a note, key, or anything else that should only be read once."></textarea>
                </label>

                <label class="field hidden" id="file-field">
                    <span>Single file</span>
                    <input id="messageFile" type="file">
                    <small>Up to 25 MB. File name and MIME type are encrypted inside the payload.</small>
                </label>

                <div class="grid two-up">
                    <label class="field">
                        <span>Six-digit PIN</span>
                        <input id="pin" name="pin" inputmode="numeric" autocomplete="one-time-code" pattern="[0-9]{6}" maxlength="6" placeholder="123456" required>
                        <small>Share this separately from the link.</small>
                    </label>

                    <label class="field">
                        <span>Expires after</span>
                        <select id="ttlPreset" name="ttlPreset"></select>
                    </label>
                </div>

                <label class="field hidden" id="custom-ttl-field">
                    <span>Custom expiry in minutes</span>
                    <input id="customTtlMinutes" type="number" min="1" max="1440" step="1" value="15">
                </label>

                <div class="status-row">
                    <button class="primary-action" id="submit-button" type="submit">Generate secure link</button>
                    <p class="inline-note">The upload contains ciphertext only.</p>
                </div>
            </div>

            <div id="create-status" class="feedback" aria-live="polite"></div>

            <div class="result-ready hidden" id="result-ready">
                <div class="result-header">
                    <p class="eyebrow">Link ready</p>
                    <h2>Deliver in two channels</h2>
                </div>

                <label class="field compact">
                    <span>Secure URL</span>
                    <textarea id="share-url" rows="4" readonly></textarea>
                </label>

                <div class="action-row">
                    <button class="secondary-action" type="button" id="copy-url-button">Copy link</button>
                    <span id="expiry-label" class="inline-note"></span>
                </div>
            </div>
        </form>
    </section>
</main>
<script type="module" src="/static/app.js"></script>
</body>
</html>
