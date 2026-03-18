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
                    <div class="mode-switch" role="tablist" aria-label="Payload type">
                        <button type="button" class="mode-chip is-active" data-mode="text">Text</button>
                        <button type="button" class="mode-chip" data-mode="file">File</button>
                    </div>
                </div>

                <input type="hidden" id="payloadType" name="payloadType" value="text">

                <label class="field" id="text-field">
                    <span>Secret text</span>
                    <textarea id="messageText" placeholder="Paste a note, key, or anything else that should only be read once."></textarea>
                </label>

                <label class="field hidden" id="file-field">
                    <span>Single file</span>
                    <div class="file-drop" id="file-drop">
                        <input id="messageFile" type="file" class="file-input-hidden">
                        <div class="file-drop-idle" id="file-drop-idle">
                            <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
                                <polyline points="17 8 12 3 7 8"/>
                                <line x1="12" y1="3" x2="12" y2="15"/>
                            </svg>
                            <p>Drop a file here or <strong>browse</strong></p>
                            <small>Up to 25 MB · name and type are encrypted</small>
                        </div>
                        <div class="file-drop-chosen hidden" id="file-drop-chosen">
                            <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                                <polyline points="14 2 14 8 20 8"/>
                            </svg>
                            <span class="file-chosen-name" id="chosen-file-name"></span>
                            <button type="button" class="file-clear-btn" id="file-clear-btn">Remove</button>
                        </div>
                    </div>
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
