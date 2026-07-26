import { WebSocketServer, WebSocket } from 'ws';
import { v4 as uuidv4 } from 'uuid';
import http from 'http';

const PORT = process.env.PORT || 3090;

// 创建 HTTP 服务器，用于健康检查和状态查询
const server = http.createServer((req, res) => {
  if (req.url === '/status') {
    const deviceList = [];
    for (const [id, ws] of devices.entries()) {
      deviceList.push({
        deviceId: id,
        state: ws.readyState === WebSocket.OPEN ? 'OPEN' : 'CLOSED',
        connId: ws.id,
        isAlive: ws.isAlive,
      });
    }
    const roomList = [];
    for (const [roomId, members] of rooms.entries()) {
      roomList.push({
        roomId,
        members: Array.from(members).map(m => ({
          deviceId: m.deviceId,
          role: m.role,
          state: m.readyState === WebSocket.OPEN ? 'OPEN' : 'CLOSED',
        })),
      });
    }
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({
      totalConnections: wss.clients.size,
      registeredDevices: deviceList,
      rooms: roomList,
      uptime: process.uptime(),
    }, null, 2));
    return;
  }
  // 默认返回 426 Upgrade Required（和之前行为一致）
  res.writeHead(426, { 'Content-Type': 'text/plain' });
  res.end('Upgrade Required');
});

const wss = new WebSocketServer({ server });

// 存储所有的房间和连接
// rooms[roomId] = Set of { ws, deviceId, role }
const rooms = new Map();

// Direct device registry for P2P sync signaling
// devices[deviceId] = ws
const devices = new Map();

function normalizeId(id) {
  if (!id) return id;
  return id.replace(/ /g, '+');
}

wss.on('connection', (ws, req) => {
  ws.id = uuidv4();
  ws.isAlive = true;

  // Check if this is a direct device connection
  let deviceId = null;
  if (req.url && req.url.includes('/device/')) {
    deviceId = req.url.split('/device/')[1];
  } else if (req.url && req.url.length > 1) {
    // NGINX might have stripped /ws/device/
    deviceId = req.url.substring(1).split('?')[0]; // remove leading slash and query params
  }

  if (deviceId && deviceId !== 'socket.io') {
    ws.deviceId = normalizeId(decodeURIComponent(deviceId));
    devices.set(ws.deviceId, ws);
    console.log(`[${ws.id}] Registered device via URL: ${ws.deviceId}`);
  }

  console.log(`[${new Date().toISOString()}] New connection: ${ws.id} from ${req.socket.remoteAddress}`);

  ws.on('pong', () => {
    ws.isAlive = true;
  });

  ws.on('message', (message) => {
    try {
      const data = JSON.parse(message);
      
      switch (data.type) {
        case 'join':
          handleJoin(ws, data);
          break;
        case 'register':
          if (data.deviceId) {
            ws.deviceId = normalizeId(data.deviceId);
            devices.set(ws.deviceId, ws);
            console.log(`[${ws.id}] Registered device via MSG: ${ws.deviceId}`);
          }
          break;
        case 'leave':
          handleLeave(ws);
          break;
        case 'signal':
          // 转发 WebRTC 信令 (offer, answer, ice-candidate)
          handleSignal(ws, data);
          break;
        case 'proxy':
          // 降级代理隧道：转发应用层数据流
          handleProxy(ws, data);
          break;
        case 'notify':
          handleNotify(ws, data);
          break;
        case 'ack':
          handleAck(ws, data);
          break;
        default:
          console.warn(`[${ws.id}] Unknown message type: ${data.type}`);
      }
    } catch (e) {
      console.error(`[${ws.id}] Failed to parse message`, e);
    }
  });

  ws.on('close', () => {
    console.log(`[${new Date().toISOString()}] Connection closed: ${ws.id}`);
    handleLeave(ws);
    if (ws.deviceId && devices.get(ws.deviceId) === ws) {
      devices.delete(ws.deviceId);
    }
  });
});

function handleJoin(ws, data) {
  const { roomId, deviceId, role } = data;
  if (!roomId || !deviceId) {
    ws.send(JSON.stringify({ type: 'error', message: 'Missing roomId or deviceId' }));
    return;
  }

  // 离开之前的房间
  handleLeave(ws);

  ws.roomId = roomId;
  ws.deviceId = deviceId;
  ws.role = role || 'client'; // 'host' (PC) or 'client' (Mobile)

  if (!rooms.has(roomId)) {
    rooms.set(roomId, new Set());
  }
  
  const room = rooms.get(roomId);
  room.add(ws);

  console.log(`[${ws.id}] Joined room: ${roomId} as ${ws.deviceId} (${ws.role})`);

  // 通知房间内的其他人有新设备加入
  broadcastToRoom(roomId, {
    type: 'peer_joined',
    peerId: deviceId,
    role: ws.role
  }, ws.id);

  // 告诉当前客户端房间内已有的设备
  const peers = Array.from(room)
    .filter(client => client.id !== ws.id)
    .map(client => ({ peerId: client.deviceId, role: client.role }));
    
  ws.send(JSON.stringify({
    type: 'room_info',
    roomId,
    peers
  }));
}

function handleLeave(ws) {
  if (!ws.roomId) return;
  
  const room = rooms.get(ws.roomId);
  if (room) {
    room.delete(ws);
    console.log(`[${ws.id}] Left room: ${ws.roomId}`);
    
    // 通知其他人
    broadcastToRoom(ws.roomId, {
      type: 'peer_left',
      peerId: ws.deviceId
    }, ws.id);

    if (room.size === 0) {
      rooms.delete(ws.roomId);
      console.log(`Room deleted: ${ws.roomId}`);
    }
  }
  
  ws.roomId = null;
}

function handleSignal(ws, data) {
  if (!ws.roomId) return;
  const { targetId, payload } = data;
  
  // 查找目标设备
  const room = rooms.get(ws.roomId);
  if (!room) return;

  for (const client of room) {
    if (client.deviceId === targetId && client.readyState === WebSocket.OPEN) {
      client.send(JSON.stringify({
        type: 'signal',
        senderId: ws.deviceId,
        payload
      }));
      return;
    }
  }
}

function handleProxy(ws, data) {
  const target_device_id = normalizeId(data.target_device_id);
  const payload = data.payload;
  const targetWs = devices.get(target_device_id);
  
  if (targetWs && targetWs.readyState === WebSocket.OPEN) {
    targetWs.send(JSON.stringify({
      type: 'proxy',
      from_device_id: ws.deviceId,
      payload: payload || {}
    }));
  } else {
    console.warn(`[${ws.id}] Proxy failed: target ${target_device_id} not found or offline`);
    // Send error back to sender so they don't wait forever
    ws.send(JSON.stringify({
      type: 'proxy_error',
      target_device_id: target_device_id,
      message: 'Target device offline'
    }));
  }
}

function handleNotify(ws, data) {
  const target_device_id = normalizeId(data.target_device_id);
  const payload = data.payload;
  const targetWs = devices.get(target_device_id);
  
  if (targetWs && targetWs.readyState === WebSocket.OPEN) {
    targetWs.send(JSON.stringify({
      type: 'notify',
      from_device_id: ws.deviceId,
      payload: payload || {}
    }));
  } else {
    console.warn(`[${ws.id}] Notify failed: target ${target_device_id} not found or offline`);
  }
}

function handleAck(ws, data) {
  const target_device_id = normalizeId(data.target_device_id);
  const targetWs = devices.get(target_device_id);
  
  if (targetWs && targetWs.readyState === WebSocket.OPEN) {
    targetWs.send(JSON.stringify({
      type: 'ack',
      from_device_id: ws.deviceId
    }));
  }
}

function broadcastToRoom(roomId, messageObj, excludeWsId = null) {
  const room = rooms.get(roomId);
  if (!room) return;
  
  const messageStr = JSON.stringify(messageObj);
  for (const client of room) {
    if (client.id !== excludeWsId && client.readyState === WebSocket.OPEN) {
      client.send(messageStr);
    }
  }
}

// 心跳检测，清理死连接
const interval = setInterval(() => {
  wss.clients.forEach((ws) => {
    if (ws.isAlive === false) {
      console.log(`[${ws.id}] Terminating dead connection`);
      handleLeave(ws);
      return ws.terminate();
    }
    
    ws.isAlive = false;
    ws.ping();
  });
}, 30000);

wss.on('close', () => {
  clearInterval(interval);
});

server.listen(PORT, () => {
  console.log(`[Bob-Relay] Signaling server running on port ${PORT}`);
});
