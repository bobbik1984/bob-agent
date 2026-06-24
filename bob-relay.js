const http = require('http');
const { WebSocketServer, WebSocket } = require('ws');

const server = http.createServer((req, res) => {
    res.writeHead(200, { 'Content-Type': 'text/plain' });
    res.end('Bob Relay OK\n');
});

const wss = new WebSocketServer({ noServer: true });
const rooms = new Map();

server.on('upgrade', (request, socket, head) => {
    const url = request.url;
    const sendMatch = url.match(/^\/ws\/send\/(.+)$/);
    const recvMatch = url.match(/^\/ws\/recv\/(.+)$/);

    if (!sendMatch && !recvMatch) {
        socket.destroy();
        return;
    }

    wss.handleUpgrade(request, socket, head, (ws) => {
        if (sendMatch) {
            const roomId = sendMatch[1];
            console.log(`🔵 Sender connected to room: ${roomId}`);
            rooms.set(roomId, { sendWs: ws, recvWs: null });

            ws.on('message', (data, isBinary) => {
                const room = rooms.get(roomId);
                if (room && room.recvWs && room.recvWs.readyState === WebSocket.OPEN) {
                    room.recvWs.send(data, { binary: isBinary });
                }
            });

            ws.on('close', () => {
                console.log(`🔴 Sender disconnected from room: ${roomId}`);
                const room = rooms.get(roomId);
                if (room && room.recvWs) {
                    room.recvWs.close();
                }
                rooms.delete(roomId);
            });

            ws.on('error', (err) => {
                console.log(`❌ Sender error in room ${roomId}: ${err.message}`);
                rooms.delete(roomId);
            });

        } else if (recvMatch) {
            const roomId = recvMatch[1];
            console.log(`🟢 Receiver connected to room: ${roomId}`);

            const room = rooms.get(roomId);
            if (!room) {
                console.log(`⚠️ Room not found: ${roomId}`);
                ws.close(1001, 'Room not found');
                return;
            }

            room.recvWs = ws;

            // Notify sender that receiver is ready
            if (room.sendWs && room.sendWs.readyState === WebSocket.OPEN) {
                room.sendWs.send('READY');
                console.log(`✅ Sent READY to sender in room: ${roomId}`);
            }

            ws.on('message', (data, isBinary) => {
                // Forward receiver messages to sender (if needed)
                if (room.sendWs && room.sendWs.readyState === WebSocket.OPEN) {
                    room.sendWs.send(data, { binary: isBinary });
                }
            });

            ws.on('close', () => {
                console.log(`🔴 Receiver disconnected from room: ${roomId}`);
                const currentRoom = rooms.get(roomId);
                if (currentRoom) currentRoom.recvWs = null;
            });

            ws.on('error', (err) => {
                console.log(`❌ Receiver error in room ${roomId}: ${err.message}`);
            });
        }
    });
});

server.listen(3900, '0.0.0.0', () => {
    console.log('🚀 Bob Relay Server listening on ws://0.0.0.0:3900');
});
