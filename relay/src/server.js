import { WebSocketServer, WebSocket } from 'ws';
import { randomUUID } from 'crypto';
import http from 'http';
import { fileURLToPath } from 'url';

const DEFAULT_PORT = Number(process.env.PORT || 3090);
const PROTOCOL_VERSION = 2;

function normalizeId(id) {
  return id ? id.replace(/ /g, '+') : id;
}

function isV2Message(data) {
  return Number(data?.protocol_version) >= PROTOCOL_VERSION
    && typeof data?.trace_id === 'string'
    && data.trace_id.length > 0
    && typeof data?.message_id === 'string'
    && data.message_id.length > 0;
}

function traceFields(data) {
  return {
    protocol_version: PROTOCOL_VERSION,
    trace_id: data.trace_id,
    message_id: data.message_id,
    ...(data.sync_id ? { sync_id: data.sync_id } : {}),
  };
}

function sendJson(ws, message, callback) {
  if (ws.readyState !== WebSocket.OPEN) {
    callback?.(new Error('socket_not_open'));
    return false;
  }
  ws.send(JSON.stringify(message), callback);
  return true;
}

function sendReceipt(ws, data, receipt, status = 'success', extra = {}) {
  if (!isV2Message(data)) return;
  sendJson(ws, {
    type: 'diagnostic_receipt',
    ...traceFields(data),
    receipt,
    status,
    timestamp: Date.now(),
    from_device_id: normalizeId(data.from_device_id),
    target_device_id: normalizeId(data.target_device_id),
    ...extra,
  });
}

export function createRelayServer({ port = DEFAULT_PORT, logger = console, faults = {} } = {}) {
  const rooms = new Map();
  const devices = new Map();

  const server = http.createServer((req, res) => {
    if (req.url === '/status') {
      const registeredDevices = Array.from(devices, ([deviceId, ws]) => ({
        deviceId,
        state: ws.readyState === WebSocket.OPEN ? 'OPEN' : 'CLOSED',
        connId: ws.id,
        isAlive: ws.isAlive,
        protocolVersion: ws.protocolVersion || 1,
      }));
      const roomList = Array.from(rooms, ([roomId, members]) => ({
        roomId,
        members: Array.from(members).map((member) => ({
          deviceId: member.deviceId,
          role: member.role,
          state: member.readyState === WebSocket.OPEN ? 'OPEN' : 'CLOSED',
        })),
      }));
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({
        protocolVersion: PROTOCOL_VERSION,
        totalConnections: wss.clients.size,
        registeredDevices,
        rooms: roomList,
        uptime: process.uptime(),
      }, null, 2));
      return;
    }
    res.writeHead(426, { 'Content-Type': 'text/plain' });
    res.end('Upgrade Required');
  });

  const wss = new WebSocketServer({ server });

  function registerDevice(ws, data) {
    if (!data.deviceId) return;
    const previousId = ws.deviceId;
    ws.deviceId = normalizeId(data.deviceId);
    ws.protocolVersion = Number(data.protocol_version) || 1;
    if (previousId && previousId !== ws.deviceId && devices.get(previousId) === ws) devices.delete(previousId);
    devices.set(ws.deviceId, ws);
    logger.log(`[${ws.id}] Registered device via MSG: ${ws.deviceId}`);
    if (isV2Message(data)) {
      sendReceipt(ws, { ...data, from_device_id: ws.deviceId, target_device_id: ws.deviceId }, 'relay_registration_accepted');
    }
  }

  function handleJoin(ws, data) {
    const { roomId, deviceId, role } = data;
    if (!roomId || !deviceId) {
      sendJson(ws, { type: 'error', message: 'Missing roomId or deviceId' });
      return;
    }
    handleLeave(ws);
    ws.roomId = roomId;
    ws.deviceId = deviceId;
    ws.role = role || 'client';
    if (!rooms.has(roomId)) rooms.set(roomId, new Set());
    const room = rooms.get(roomId);
    room.add(ws);
    broadcastToRoom(roomId, { type: 'peer_joined', peerId: deviceId, role: ws.role }, ws.id);
    const peers = Array.from(room).filter((client) => client.id !== ws.id)
      .map((client) => ({ peerId: client.deviceId, role: client.role }));
    sendJson(ws, { type: 'room_info', roomId, peers });
  }

  function handleLeave(ws) {
    if (!ws.roomId) return;
    const room = rooms.get(ws.roomId);
    if (room) {
      room.delete(ws);
      broadcastToRoom(ws.roomId, { type: 'peer_left', peerId: ws.deviceId }, ws.id);
      if (room.size === 0) rooms.delete(ws.roomId);
    }
    ws.roomId = null;
  }

  function handleSignal(ws, data) {
    if (!ws.roomId) return;
    const room = rooms.get(ws.roomId);
    if (!room) return;
    for (const client of room) {
      if (client.deviceId === data.targetId && client.readyState === WebSocket.OPEN) {
        sendJson(client, { type: 'signal', senderId: ws.deviceId, payload: data.payload });
        return;
      }
    }
  }

  function handleDirectMessage(ws, data, type) {
    const fromDeviceId = normalizeId(ws.deviceId);
    const targetDeviceId = normalizeId(data.target_device_id);
    const routedData = { ...data, from_device_id: fromDeviceId, target_device_id: targetDeviceId };
    const isResponse = data.flow_phase === 'response' || type === 'ack';

    const acceptedReceipt = isResponse ? 'relay_response_accepted' : 'relay_request_accepted';
    const deliveredReceipt = isResponse ? 'relay_delivered_to_origin' : 'relay_delivered_to_target';
    const sendAccepted = () => sendReceipt(ws, routedData, acceptedReceipt);
    if (!faults.outOfOrderReceipts) sendAccepted();
    const targetWs = devices.get(targetDeviceId);
    if (!targetWs || targetWs.readyState !== WebSocket.OPEN) {
      logger.warn(`[${ws.id}] ${type} failed: target ${targetDeviceId} not found or offline`);
      if (isV2Message(data)) {
        if (faults.outOfOrderReceipts) sendAccepted();
        sendReceipt(ws, routedData, 'target_offline', 'failed', { error_code: 'RLY-TARGET-OFFLINE' });
      } else if (type === 'proxy') {
        sendJson(ws, { type: 'proxy_error', target_device_id: targetDeviceId, message: 'Target device offline' });
      }
      return;
    }

    const forwarded = {
      type,
      from_device_id: fromDeviceId,
      ...(type !== 'ack' ? { payload: data.payload || {} } : {}),
      ...(isV2Message(data) ? { ...traceFields(data), flow_phase: isResponse ? 'response' : 'request' } : {}),
    };

    const shouldDrop = isResponse
      ? faults.dropResponseBeforeOriginDelivery
      : faults.dropRequestBeforeTargetDelivery;
    if (shouldDrop) {
      sendReceipt(ws, routedData, 'delivery_failed', 'failed', { error_code: 'RLY-FAULT-INJECTED' });
      return;
    }

    const deliver = () => sendJson(targetWs, forwarded, (error) => {
      if (error) {
        logger.warn(`[${ws.id}] ${type} delivery failed: ${error.message}`);
        sendReceipt(ws, routedData, 'delivery_failed', 'failed', { error_code: 'RLY-DELIVERY-FAILED' });
        return;
      }
      sendReceipt(ws, routedData, deliveredReceipt);
      if (faults.duplicateDeliveryReceipt) sendReceipt(ws, routedData, deliveredReceipt);
      if (faults.outOfOrderReceipts) sendAccepted();
    });

    const delayMs = Math.max(0, Number(faults.deliveryDelayMs) || 0);
    if (delayMs > 0) setTimeout(deliver, delayMs);
    else deliver();
  }

  function broadcastToRoom(roomId, message, excludedId = null) {
    const room = rooms.get(roomId);
    if (!room) return;
    for (const client of room) if (client.id !== excludedId) sendJson(client, message);
  }

  wss.on('connection', (ws, req) => {
    ws.id = randomUUID();
    ws.isAlive = true;
    let deviceId = null;
    if (req.url?.includes('/device/')) deviceId = req.url.split('/device/')[1];
    else if (req.url?.length > 1) deviceId = req.url.substring(1).split('?')[0];
    if (deviceId && deviceId !== 'socket.io') {
      ws.deviceId = normalizeId(decodeURIComponent(deviceId));
      devices.set(ws.deviceId, ws);
      logger.log(`[${ws.id}] Registered device via URL: ${ws.deviceId}`);
    }

    logger.log(`[${new Date().toISOString()}] New connection: ${ws.id} from ${req.socket.remoteAddress}`);
    ws.on('pong', () => { ws.isAlive = true; });
    ws.on('message', (message) => {
      try {
        const data = JSON.parse(message);
        switch (data.type) {
          case 'join': handleJoin(ws, data); break;
          case 'register': registerDevice(ws, data); break;
          case 'leave': handleLeave(ws); break;
          case 'signal': handleSignal(ws, data); break;
          case 'proxy': handleDirectMessage(ws, data, 'proxy'); break;
          case 'notify': handleDirectMessage(ws, data, 'notify'); break;
          case 'wakeup': handleDirectMessage(ws, data, 'wakeup'); break;
          case 'ack': handleDirectMessage(ws, data, 'ack'); break;
          default: logger.warn(`[${ws.id}] Unknown message type: ${data.type}`);
        }
      } catch (error) {
        logger.error(`[${ws.id}] Failed to parse message`, error);
      }
    });
    ws.on('close', () => {
      handleLeave(ws);
      if (ws.deviceId && devices.get(ws.deviceId) === ws) devices.delete(ws.deviceId);
      logger.log(`[${new Date().toISOString()}] Connection closed: ${ws.id}`);
    });
  });

  const heartbeat = setInterval(() => {
    wss.clients.forEach((ws) => {
      if (ws.isAlive === false) {
        handleLeave(ws);
        return ws.terminate();
      }
      ws.isAlive = false;
      ws.ping();
    });
  }, 30000);

  async function start() {
    await new Promise((resolve, reject) => {
      server.once('error', reject);
      server.listen(port, () => {
        server.off('error', reject);
        resolve();
      });
    });
    logger.log(`[Bob-Relay] Signaling server running on port ${server.address().port}`);
    return server.address();
  }

  async function stop() {
    clearInterval(heartbeat);
    for (const client of wss.clients) client.terminate();
    await new Promise((resolve) => wss.close(() => server.close(resolve)));
  }

  return { server, wss, devices, rooms, start, stop };
}

const isMainModule = process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1];
if (isMainModule) {
  createRelayServer().start().catch((error) => {
    console.error('[Bob-Relay] Failed to start', error);
    process.exitCode = 1;
  });
}
