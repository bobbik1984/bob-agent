import asyncio
import websockets

async def test_ws():
    print("Testing wss://relay.bobbik.org")
    try:
        async with websockets.connect("wss://relay.bobbik.org") as ws:
            print("Successfully connected!")
            await ws.send('{"type": "ping"}')
            response = await asyncio.wait_for(ws.recv(), timeout=5)
            print("Received:", response)
    except Exception as e:
        print("Failed:", e)

asyncio.run(test_ws())
