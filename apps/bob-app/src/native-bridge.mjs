import { invoke, isTauri } from '@tauri-apps/api/core';
import { createKernelBridge } from './kernel-bridge.mjs';
import { httpInvoke, isBobNodeRuntime } from './http-bridge.mjs';

function isHostAvailable() {
  return Boolean(isTauri() || isBobNodeRuntime());
}

function unifiedInvoke(command, args) {
  if (isTauri()) {
    return invoke(command, args);
  }
  return httpInvoke(command, args);
}

export const kernelBridge = createKernelBridge(unifiedInvoke, isHostAvailable);
