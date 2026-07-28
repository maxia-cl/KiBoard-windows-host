// Real Tauri bridge for device management (F1) — unlike the rest of this app (still MockBridge
// until F5), this talks straight to the compiled host via window.__TAURI__ (withGlobalTauri in
// tauri.conf.json). Only meaningful when running inside the actual Tauri window, not `vite dev`
// in a plain browser.
function invoke(cmd, args) {
  const api = typeof window !== "undefined" ? window.__TAURI__?.core : undefined;
  if (!api) {
    return Promise.reject(new Error("Not running inside the Tauri window"));
  }
  return api.invoke(cmd, args);
}

export function isTauri() {
  return typeof window !== "undefined" && !!window.__TAURI__;
}

export function pairingStatus() {
  return invoke("pairing_status");
}

export function listDevices() {
  return invoke("list_devices");
}

export function revokeDevice(deviceId) {
  return invoke("revoke_device", { deviceId });
}

export function setPairingOpen(open) {
  return invoke("set_pairing_open", { open });
}
