import asyncio
import websockets
import json

async def run_test():
    test_id = "vK6sX7vjSjfhe+joRJUFSplx8g2R8Hzj+x3RGANbgI0="
    try:
        async with websockets.connect(f"wss://relay.bobbik.org/ws/device/{test_id}") as ws:
            await ws.send(json.dumps({"type": "register", "deviceId": test_id}))
            print("Successfully connected and registered with + in ID!")
            
            # send to self
            notify = {
                "type": "notify",
                "target_device_id": test_id,
                "payload": {"device_name": "MyTestPhone"}
            }
            await ws.send(json.dumps(notify))
            print("Sent notify to self")
            msg = await asyncio.wait_for(ws.recv(), timeout=2.0)
            print("Received:", msg)
    except Exception as e:
        print("Failed:", e)

asyncio.run(run_test())
