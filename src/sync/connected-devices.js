export function displayConnectedDevices(devices, localDeviceId, isNativeMobile) {
  return (Array.isArray(devices) ? devices : []).filter((device) => {
    if (!device || device.device_id === localDeviceId || device.status === 'revoked') return false;
    return !isNativeMobile || device.is_trusted === true || device.status === 'trusted';
  });
}

export function hasActivePairedDevice(devices, localDeviceId) {
  return displayConnectedDevices(devices, localDeviceId, true).length > 0;
}
