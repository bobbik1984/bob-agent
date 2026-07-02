import os
import subprocess
import sys

def run_cmd(cmd):
    print(f"Running: {cmd}")
    res = subprocess.run(cmd, shell=True)
    if res.returncode != 0:
        print(f"Failed with exit code {res.returncode}")
        sys.exit(res.returncode)

def main():
    print("Deploying bob_relay.py to VPS3 (Contabo_VPS)...")
    
    # Check if bob_relay.py exists locally
    script_path = "deploy/bob_relay.py"
    if not os.path.exists(script_path):
        print(f"Python script not found at {script_path}")
        sys.exit(1)
        
    # First, make sure destination directory exists
    run_cmd('plink -batch -load "Contabo_VPS" "mkdir -p /home/ubuntu/bob-relay"')
    
    # Install fastapi if missing
    print("Installing fastapi on VPS3...")
    run_cmd('plink -batch -load "Contabo_VPS" "/opt/quant_lab/.venv/bin/pip install fastapi"')

    # Push python script
    print("Pushing bob_relay.py...")
    run_cmd(f'pscp -load "Contabo_VPS" {script_path} ubuntu@89.117.22.194:/home/ubuntu/bob-relay/bob_relay.py')

    # Service configuration
    service_content = """[Unit]
Description=Bob Relay Server (WebSocket & Proxy) Python
After=network.target

[Service]
Type=simple
User=ubuntu
WorkingDirectory=/home/ubuntu/bob-relay
ExecStart=/opt/quant_lab/.venv/bin/python bob_relay.py
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
"""
    with open("bob-relay.service", "w", encoding="utf-8") as f:
        f.write(service_content)

    print("Pushing systemd service file...")
    run_cmd('pscp -load "Contabo_VPS" bob-relay.service ubuntu@89.117.22.194:/tmp/bob-relay.service')
    run_cmd('plink -batch -load "Contabo_VPS" "sudo cp /tmp/bob-relay.service /etc/systemd/system/bob-relay.service && sudo systemctl daemon-reload && sudo systemctl enable bob-relay"')
    
    # Restart service 
    print("Restarting bob-relay service...")
    run_cmd('plink -batch -load "Contabo_VPS" "sudo systemctl restart bob-relay"')
    
    print("Deployment completed successfully!")

if __name__ == "__main__":
    main()

