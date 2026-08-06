import test from 'node:test';
import assert from 'node:assert/strict';
import WebSocket from 'ws';
import { createRelayServer } from '../src/server.js';

const silentLogger = { log() {}, warn() {}, error() {} };

function nextMessage(ws, predicate = () => true, timeoutMs = 1500) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      ws.off('message', onMessage);
      reject(new Error('message timeout'));
    }, timeoutMs);
    const onMessage = (raw) => {
      const message = JSON.parse(raw);
      if (!predicate(message)) return;
      clearTimeout(timer);
      ws.off('message', onMessage);
      resolve(message);
    };
    ws.on('message', onMessage);
  });
}

function openSocket(url) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(url);
    ws.once('open', () => resolve(ws));
    ws.once('error', reject);
  });
}

async function fixture(t, faults = {}) {
  const relay = createRelayServer({ port: 0, logger: silentLogger, faults });
  const address = await relay.start();
  const sockets = [];
  t.after(async () => {
    for (const ws of sockets) ws.terminate();
    await relay.stop();
  });
  return {
    relay,
    async connect(deviceId) {
      const ws = await openSocket(`ws://127.0.0.1:${address.port}/ws/device/${deviceId}`);
      sockets.push(ws);
      return ws;
    },
  };
}

function v2(overrides = {}) {
  return {
    protocol_version: 2,
    trace_id: 'trace-1',
    message_id: 'message-1',
    sync_id: 'sync-1',
    ...overrides,
  };
}

test('v1 proxy remains compatible and receives no diagnostic receipt', async (t) => {
  const fx = await fixture(t);
  const mobile = await fx.connect('mobile');
  const pc = await fx.connect('pc');
  const forwarded = nextMessage(pc, (message) => message.type === 'proxy');
  mobile.send(JSON.stringify({ type: 'proxy', target_device_id: 'pc', payload: { action: 'pull' } }));
  assert.deepEqual(await forwarded, { type: 'proxy', from_device_id: 'mobile', payload: { action: 'pull' } });
});

test('v1 wakeup is routed to the target device', async (t) => {
  const fx = await fixture(t);
  const mobile = await fx.connect('mobile');
  const pc = await fx.connect('pc');
  const forwarded = nextMessage(pc, (message) => message.type === 'wakeup');
  mobile.send(JSON.stringify({ type: 'wakeup', target_device_id: 'pc' }));
  assert.deepEqual(await forwarded, { type: 'wakeup', from_device_id: 'mobile', payload: {} });
});

test('v2 registration returns an explicit acceptance receipt', async (t) => {
  const fx = await fixture(t);
  const mobile = await fx.connect('mobile');
  const receipt = nextMessage(mobile, (message) => message.receipt === 'relay_registration_accepted');
  mobile.send(JSON.stringify({ type: 'register', deviceId: 'mobile', ...v2() }));
  const message = await receipt;
  assert.equal(message.status, 'success');
  assert.equal(message.trace_id, 'trace-1');
});

test('v2 request receives accepted and delivered receipts with trace fields', async (t) => {
  const fx = await fixture(t);
  const mobile = await fx.connect('mobile');
  const pc = await fx.connect('pc');
  const receipts = [];
  mobile.on('message', (raw) => {
    const message = JSON.parse(raw);
    if (message.type === 'diagnostic_receipt') receipts.push(message);
  });
  const forwarded = nextMessage(pc, (message) => message.type === 'proxy');
  mobile.send(JSON.stringify({ type: 'proxy', target_device_id: 'pc', payload: { action: 'pull' }, ...v2() }));
  const targetMessage = await forwarded;
  assert.equal(targetMessage.trace_id, 'trace-1');
  assert.equal(targetMessage.message_id, 'message-1');
  await new Promise((resolve) => setTimeout(resolve, 30));
  assert.deepEqual(receipts.map((item) => item.receipt), ['relay_request_accepted', 'relay_delivered_to_target']);
});

test('v2 offline target returns a stable error instead of a silent timeout', async (t) => {
  const fx = await fixture(t);
  const mobile = await fx.connect('mobile');
  const failed = nextMessage(mobile, (message) => message.receipt === 'target_offline');
  mobile.send(JSON.stringify({ type: 'notify', target_device_id: 'missing-pc', payload: {}, ...v2() }));
  const message = await failed;
  assert.equal(message.status, 'failed');
  assert.equal(message.error_code, 'RLY-TARGET-OFFLINE');
  assert.equal(message.target_device_id, 'missing-pc');
});

test('v2 response uses the return-path receipts', async (t) => {
  const fx = await fixture(t);
  const mobile = await fx.connect('mobile');
  const pc = await fx.connect('pc');
  const receipts = [];
  pc.on('message', (raw) => {
    const message = JSON.parse(raw);
    if (message.type === 'diagnostic_receipt') receipts.push(message.receipt);
  });
  const ack = nextMessage(mobile, (message) => message.type === 'ack');
  pc.send(JSON.stringify({ type: 'ack', target_device_id: 'mobile', flow_phase: 'response', ...v2({ message_id: 'response-1' }) }));
  await ack;
  await new Promise((resolve) => setTimeout(resolve, 30));
  assert.deepEqual(receipts, ['relay_response_accepted', 'relay_delivered_to_origin']);
});

test('fault injection can isolate request-path loss before target delivery', async (t) => {
  const fx = await fixture(t, { dropRequestBeforeTargetDelivery: true });
  const mobile = await fx.connect('mobile');
  await fx.connect('pc');
  const failed = nextMessage(mobile, (message) => message.receipt === 'delivery_failed');
  mobile.send(JSON.stringify({ type: 'proxy', target_device_id: 'pc', payload: {}, ...v2() }));
  const message = await failed;
  assert.equal(message.error_code, 'RLY-FAULT-INJECTED');
  assert.equal(message.from_device_id, 'mobile');
  assert.equal(message.target_device_id, 'pc');
});

test('fault injection can isolate response-path loss before origin delivery', async (t) => {
  const fx = await fixture(t, { dropResponseBeforeOriginDelivery: true });
  await fx.connect('mobile');
  const pc = await fx.connect('pc');
  const failed = nextMessage(pc, (message) => message.receipt === 'delivery_failed');
  pc.send(JSON.stringify({ type: 'ack', target_device_id: 'mobile', flow_phase: 'response', ...v2() }));
  const message = await failed;
  assert.equal(message.error_code, 'RLY-FAULT-INJECTED');
  assert.equal(message.from_device_id, 'pc');
  assert.equal(message.target_device_id, 'mobile');
});

test('delivery delay is deterministic and still preserves trace fields', async (t) => {
  const fx = await fixture(t, { deliveryDelayMs: 60 });
  const mobile = await fx.connect('mobile');
  const pc = await fx.connect('pc');
  const startedAt = Date.now();
  const forwarded = nextMessage(pc, (message) => message.type === 'notify');
  mobile.send(JSON.stringify({ type: 'notify', target_device_id: 'pc', payload: {}, ...v2() }));
  const message = await forwarded;
  assert.ok(Date.now() - startedAt >= 45);
  assert.equal(message.trace_id, 'trace-1');
});

test('duplicate delivery receipts retain the same idempotency key', async (t) => {
  const fx = await fixture(t, { duplicateDeliveryReceipt: true });
  const mobile = await fx.connect('mobile');
  await fx.connect('pc');
  const deliveries = [];
  mobile.on('message', (raw) => {
    const message = JSON.parse(raw);
    if (message.receipt === 'relay_delivered_to_target') deliveries.push(message);
  });
  mobile.send(JSON.stringify({ type: 'notify', target_device_id: 'pc', payload: {}, ...v2() }));
  await new Promise((resolve) => setTimeout(resolve, 60));
  assert.equal(deliveries.length, 2);
  assert.equal(deliveries[0].message_id, deliveries[1].message_id);
  assert.equal(deliveries[0].trace_id, deliveries[1].trace_id);
});

test('out-of-order receipts are observable without changing delivery', async (t) => {
  const fx = await fixture(t, { outOfOrderReceipts: true });
  const mobile = await fx.connect('mobile');
  await fx.connect('pc');
  const receipts = [];
  mobile.on('message', (raw) => {
    const message = JSON.parse(raw);
    if (message.type === 'diagnostic_receipt') receipts.push(message.receipt);
  });
  mobile.send(JSON.stringify({ type: 'notify', target_device_id: 'pc', payload: {}, ...v2() }));
  await new Promise((resolve) => setTimeout(resolve, 60));
  assert.deepEqual(receipts, ['relay_delivered_to_target', 'relay_request_accepted']);
});
