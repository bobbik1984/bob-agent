import { invoke, isTauri } from '@tauri-apps/api/core';
import { createKernelBridge } from './kernel-bridge.mjs';
export const kernelBridge = createKernelBridge(invoke, isTauri);
