// Device management (F1). Shares the one `invoke` helper with the rest of the editor — F5 moved
// it into bridge.js, when the editor stopped being a mock and needed the same door.
import { invoke, isTauri } from "./bridge.js";

export { isTauri };

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

export function setManualEnabled(enabled) {
  return invoke("set_manual_enabled", { enabled });
}
