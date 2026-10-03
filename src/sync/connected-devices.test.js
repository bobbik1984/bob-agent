import { describe, expect, it } from 'vitest';
import { displayConnectedDevices, hasActivePairedDevice } from './connected-devices.js';

const local = { device_id: 'phone', status: 'trusted', is_trusted: true };
const pc = { device_id: 'pc', status: 'trusted', is_trusted: true };
const revoked = { device_id: 'old-pc', status: 'revoked', is_trusted: false };
const discovered = { device_id: 'candidate', status: 'untrusted', is_trusted: false };

describe('mobile connected-device actions', () => {
  it('shows only active trusted peers on mobile and hides unbind after revocation', () => {
    expect(displayConnectedDevices([local, pc, revoked, discovered], 'phone', true)).toEqual([pc]);
    expect(displayConnectedDevices([local, revoked, discovered], 'phone', true)).toEqual([]);
    expect(hasActivePairedDevice([local, revoked, discovered], 'phone')).toBe(false);
  });

  it('retains desktop discovery candidates but never offers revoked entries', () => {
    expect(displayConnectedDevices([local, pc, revoked, discovered], 'phone', false)).toEqual([pc, discovered]);
    expect(hasActivePairedDevice([pc], 'phone')).toBe(true);
  });
});
