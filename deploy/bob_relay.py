import asyncio
from fastapi import FastAPI, Request, WebSocket, WebSocketDisconnect
from fastapi.responses import StreamingResponse
import httpx
import uvicorn
import logging
from typing import Dict, List

app = FastAPI()
logger = logging.getLogger("bob-relay")
logging.basicConfig(level=logging.INFO)

class Room:
    def __init__(self):
        self.queues: List[asyncio.Queue] = []
        self.sender_ws: WebSocket = None
        self.receiver_connected = asyncio.Event()

rooms: Dict[str, Room] = {}

@app.websocket("/ws/send/{room_id}")
async def ws_send_handler(websocket: WebSocket, room_id: str):
    await websocket.accept()
    room = Room()
    room.sender_ws = websocket
    rooms[room_id] = room
    logger.info(f"🔵 Sender connected to room: {room_id}")

    async def wait_receiver():
        await room.receiver_connected.wait()
        try:
            await websocket.send_text("READY")
        except:
            pass

    wait_task = asyncio.create_task(wait_receiver())

    try:
        while True:
            message = await websocket.receive()
            if "text" in message:
                for q in room.queues:
                    await q.put({"type": "text", "data": message["text"]})
            elif "bytes" in message:
                for q in room.queues:
                    await q.put({"type": "bytes", "data": message["bytes"]})
    except WebSocketDisconnect:
        pass
    finally:
        wait_task.cancel()
        if room_id in rooms:
            del rooms[room_id]
        logger.info(f"🔴 Sender disconnected, destroying room: {room_id}")

@app.websocket("/ws/recv/{room_id}")
async def ws_recv_handler(websocket: WebSocket, room_id: str):
    if room_id not in rooms:
        logger.warning(f"⚠️ Room not found: {room_id}")
        await websocket.close()
        return

    await websocket.accept()
    logger.info(f"🟢 Receiver connected to room: {room_id}")
    room = rooms[room_id]
    q = asyncio.Queue()
    room.queues.append(q)
    room.receiver_connected.set()

    try:
        while True:
            msg = await q.get()
            if msg["type"] == "text":
                await websocket.send_text(msg["data"])
            elif msg["type"] == "bytes":
                await websocket.send_bytes(msg["data"])
    except WebSocketDisconnect:
        logger.info(f"🔴 Receiver disconnected from room: {room_id}")
    finally:
        if q in room.queues:
            room.queues.remove(q)

@app.post("/api/proxy")
@app.get("/api/proxy")
async def proxy_handler(request: Request):
    target_url = request.headers.get("X-Proxy-Target-Url")
    if not target_url:
        return StreamingResponse(iter([]), status_code=400)
    
    target_method = request.headers.get("X-Proxy-Target-Method", "GET").upper()

    headers = {}
    for k, v in request.headers.items():
        if k.lower().startswith("x-proxy-pass-"):
            real_key = k[len("x-proxy-pass-"):]
            headers[real_key] = v

    body_bytes = await request.body()
    
    client = httpx.AsyncClient()
    req = client.build_request(
        method=target_method,
        url=target_url,
        headers=headers,
        content=body_bytes if body_bytes else None,
        timeout=120.0
    )

    try:
        resp = await client.send(req, stream=True)
        
        async def stream_generator():
            async for chunk in resp.aiter_bytes():
                yield chunk
            await client.aclose()
            
        return StreamingResponse(
            stream_generator(),
            status_code=resp.status_code,
            headers={k: v for k, v in resp.headers.items() if k.lower() not in ("content-encoding", "content-length", "transfer-encoding")}
        )
    except Exception as e:
        await client.aclose()
        return StreamingResponse(iter([str(e).encode()]), status_code=502)

if __name__ == "__main__":
    uvicorn.run("bob_relay:app", host="0.0.0.0", port=3900, reload=False)
