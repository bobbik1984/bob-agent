import asyncio
import websockets
import json

async def run_test():
    async def pc_client():
        async with websockets.connect("wss://relay.bobbik.org/ws/device/PC_TEST") as ws:
            await ws.send(json.dumps({"type": "register", "deviceId": "PC_TEST"}))
            print("PC connected and registered.")
            msg = await ws.recv()
            data = json.loads(msg)
            print("PC received:", msg)
            if data.get("type") == "notify":
                ack = {
                    "type": "ack",
                    "target_device_id": data["from_device_id"]
                }
                await ws.send(json.dumps(ack))
                print("PC sent ack.")

    async def mobile_client():
        await asyncio.sleep(1) # wait for PC
        async with websockets.connect("wss://relay.bobbik.org/ws/device/MOBILE_TEST") as ws:
            await ws.send(json.dumps({"type": "register", "deviceId": "MOBILE_TEST"}))
            print("MOBILE connected and registered.")
            notify = {
                "type": "notify",
                "target_device_id": "PC_TEST",
                "payload": {"device_name": "MyPhone"}
            }
            await ws.send(json.dumps(notify))
            print("MOBILE sent notify.")
            ack_msg = await asyncio.wait_for(ws.recv(), timeout=5.0)
            print("MOBILE received:", ack_msg)

    await asyncio.gather(pc_client(), mobile_client())

asyncio.run(run_test())
