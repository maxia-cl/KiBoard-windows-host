<script>
  import { onMount, onDestroy } from "svelte";
  import {
    isTauri,
    pairingStatus,
    listDevices,
    revokeDevice,
    setPairingOpen,
    setManualEnabled,
  } from "./pairing.js";
  import { t } from "./i18n.js";

  // `firstRun` is set when the editor opened this panel by itself because nothing has ever paired
  // (B1). Everything below is the same panel — what changes is that it explains the three steps
  // instead of assuming the reader already knows them.
  let { onclose, onmanualchange = () => {}, firstRun = false } = $props();

  let status = $state(null); // { hostId, pairingOpen, pending }
  let devices = $state([]);
  let error = $state(isTauri() ? null : t("pair.nohost"));
  let showManualIntro = $state(false);
  let knownManual = null;
  let timer = null;

  async function refresh() {
    if (!isTauri()) return;
    try {
      status = await pairingStatus();
      if (knownManual !== status.manualEnabled) {
        knownManual = status.manualEnabled;
        onmanualchange(status.manualEnabled === true, false);
      }
      devices = await listDevices();
      error = null;
    } catch (e) {
      error = String(e);
    }
  }

  async function toggleOpen() {
    if (!status) return;
    await setPairingOpen(!status.pairingOpen);
    await refresh();
  }

  async function revoke(deviceId) {
    await revokeDevice(deviceId);
    await refresh();
  }

  async function toggleManual() {
    if (!status) return;
    const enabled = !status.manualEnabled;
    const result = await setManualEnabled(enabled);
    status = { ...status, manualEnabled: enabled };
    knownManual = enabled;
    onmanualchange(enabled, false);
    showManualIntro = enabled && result?.showIntro === true;
  }

  function openManualEditor() {
    showManualIntro = false;
    onmanualchange(true, true);
    onclose();
  }

  onMount(() => {
    refresh();
    // The pending code is short-lived (120s) and set from another connection (the phone) — poll
    // rather than push, this panel isn't performance-sensitive.
    timer = setInterval(refresh, 2000);
  });
  onDestroy(() => {
    if (timer) clearInterval(timer);
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onpointerdown={onclose} role="presentation">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="panel" onpointerdown={(e) => e.stopPropagation()} role="presentation">
    <div class="header">
      <span>{firstRun ? t("pair.firstrun") : t("settings.title")}</span>
      <button class="close" onclick={onclose}>×</button>
    </div>

    {#if firstRun}
      <!-- `{@html}` because these four carry a <b> that is part of the sentence. Safe: they come
           from the static table in i18n.js, never from a device name or anything off the wire. -->
      <ol class="steps">
        <li>{@html t("pair.step1")}</li>
        <li>{@html t("pair.step2")}</li>
        <li>{@html t("pair.step3")}</li>
      </ol>
      <p class="hint">{@html t("pair.firewall")}</p>
    {/if}

    {#if error}
      <p class="hint">{error}</p>
    {:else if !status}
      <p class="hint">{t("loading")}</p>
    {:else}
      <div class="section-title">{t("settings.advanced")}</div>
      <label class="feature-card">
        <span>
          <strong>{t("manual.feature")}</strong>
          <small>{t("manual.featurehint")}</small>
        </span>
        <input type="checkbox" checked={status.manualEnabled} onchange={toggleManual} />
      </label>

      {#if showManualIntro}
        <div class="manual-intro">
          <strong>{t("manual.introtitle")}</strong>
          <p>{t("manual.introbody")}</p>
          <ol>
            <li>{t("manual.intro1")}</li>
            <li>{t("manual.intro2")}</li>
            <li>{t("manual.intro3")}</li>
          </ol>
          <button class="primary" onclick={openManualEditor}>{t("manual.introcta")}</button>
        </div>
      {/if}

      <div class="section-title">{t("pairing.title")}</div>
      <div class="row">
        <span class="label">{t("pair.hostid")}</span>
        <code>{status.hostId}</code>
      </div>
      <label class="row toggle">
        <span class="label">{t("pair.accept")}</span>
        <input type="checkbox" checked={status.pairingOpen} onchange={toggleOpen} />
      </label>

      <!--
        R1's escape hatch, from this side. A network that drops multicast leaves the phone with an
        empty list, and its only way in is being told where to look — so the address sits here, next
        to the code, instead of somewhere the user has to be told how to find.
      -->
      <div class="row">
        <span class="label">{t("pair.address")}</span>
        <code>{status.ip}:{status.port}</code>
      </div>
      <p class="hint">{t("pair.addresshint")}</p>

      <!--
        §2.2. Only worth looking at when a paired phone starts refusing to connect: that is either
        this certificate having changed or somebody standing in the middle, and the phone has no
        way to tell the user which.
      -->
      {#if status.fingerprint}
        <div class="row">
          <span class="label">{t("pair.certificate")}</span>
          <code class="fingerprint">{status.fingerprint.slice(0, 16)}…</code>
        </div>
      {/if}

      {#if status.pending}
        <div class="pending">
          <div class="code">{status.pending.code}</div>
          <div class="hint">
            {t("pair.wants", status.pending.device, status.pending.expiresIn)}
          </div>
        </div>
      {/if}

      <div class="devices">
        <div class="devices-title">{t("pair.devices")}</div>
        {#if devices.length === 0}
          <p class="hint">{t("pair.nodevices")}</p>
        {/if}
        {#each devices as d (d.device_id)}
          <div class="device">
            <div>
              <div class="device-name">{d.name}</div>
              <div class="hint">{d.platform}</div>
            </div>
            <button class="revoke" onclick={() => revoke(d.device_id)}>{t("pair.revoke")}</button>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 8vh;
    z-index: 1300;
  }
  .panel {
    background: var(--deck-color-surface);
    border: 1px solid var(--deck-color-surface-border);
    border-radius: 14px;
    padding: 16px;
    width: 340px;
    max-height: 76vh;
    overflow-y: auto;
    color: var(--deck-color-text-primary);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-weight: 600;
  }
  .close {
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    font-size: 16px;
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 13px;
  }
  .toggle {
    cursor: pointer;
  }
  .label {
    color: var(--deck-color-text-secondary);
  }
  .pending {
    background: var(--deck-color-surface-raised);
    border-radius: 8px;
    padding: 10px;
    text-align: center;
  }
  .code {
    font-size: 28px;
    letter-spacing: 0.3em;
    font-weight: 700;
  }
  .devices-title {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--deck-color-text-secondary);
    margin-bottom: 6px;
  }
  .device {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 0;
    border-top: 1px solid var(--deck-color-surface-border);
  }
  .device-name {
    font-size: 13px;
  }
  .revoke {
    background: var(--deck-color-key-danger-background);
    color: white;
    border: none;
    border-radius: 6px;
    padding: 4px 10px;
    font-size: 12px;
    cursor: pointer;
  }
  .steps {
    margin: 0;
    padding-left: 18px;
    font-size: 13px;
    line-height: 1.5;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .hint {
    font-size: 12px;
    color: var(--deck-color-text-secondary);
  }
  .fingerprint {
    font-size: 11px;
    letter-spacing: 0.5px;
  }
  code {
    font-family: ui-monospace, monospace;
  }
  .section-title {
    color: var(--deck-color-text-secondary);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    padding-top: 2px;
  }
  .feature-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 12px;
    border: 1px solid var(--deck-color-surface-border);
    border-radius: 10px;
    background: var(--deck-color-surface-raised);
    cursor: pointer;
  }
  .feature-card span {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .feature-card small,
  .manual-intro p,
  .manual-intro ol {
    color: var(--deck-color-text-secondary);
    font-size: 12px;
    line-height: 1.45;
  }
  .manual-intro {
    border-left: 3px solid var(--deck-color-manual-active);
    border-radius: 8px;
    background: color-mix(in srgb, var(--deck-color-manual-active) 10%, var(--deck-color-surface-raised));
    padding: 12px;
  }
  .manual-intro p {
    margin: 6px 0;
  }
  .manual-intro ol {
    margin: 8px 0 12px;
    padding-left: 18px;
  }
  .primary {
    width: 100%;
    border: 0;
    border-radius: 8px;
    padding: 8px 12px;
    background: var(--deck-color-manual-active);
    color: white;
    font-weight: 600;
    cursor: pointer;
  }
</style>
