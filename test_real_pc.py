import asyncio
import websockets
import json
import os

async def run_test():
    # Read PC device ID
    config_path = os.path.join(os.environ['APPDATA'], 'bob.agent', 'config.json')
    with open(config_path) as f:
        config = json.load(f)
    pc_device_id = config.get('device_id')
    print("PC device_id:", pc_device_id)

    async def mobile_client():
        # Connect as mobile test device
        async with websockets.connect("wss://relay.bobbik.org/ws/device/MOBILE_TEST2") as ws:
            await ws.send(json.dumps({"type": "register", "deviceId": "MOBILE_TEST2"}))
            print("MOBILE connected and registered.")
            notify = {
                "type": "notify",
                "target_device_id": pc_device_id,
                "payload": {"device_name": "MyTestPhone"}
            }
            await ws.send(json.dumps(notify))
            print("MOBILE sent notify to", pc_device_id)
            
            try:
                ack_msg = await asyncio.wait_for(ws.recv(), timeout=5.0)
                print("MOBILE received from Relay:", ack_msg)
            except Exception as e:
                print("MOBILE failed to receive ACK:", e)

    await asyncio.gather(mobile_client())

asyncio.run(run_test())
