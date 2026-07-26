      // Step 3: 尝试局域网直连同步 (LAN Sync)
      if (window.appAPI.triggerMobileSync) {
        updateStep('lan_sync', 'running', '');
        let lanSuccess = false;
        try {
          const syncTimeout = new Promise((_, reject) => setTimeout(() => reject(new Error('Sync Timeout')), 15000));
          
          // Force LAN only for this attempt
          const lanPayload = { ...payload, skip_relay: true };
          
          await Promise.race([
            window.appAPI.triggerMobileSync(lanPayload),
            syncTimeout
          ]);
          
          const lanStep = pairingSteps.value.find(s => s.id === 'lan_sync');
          if (lanStep && (lanStep.status === 'done' || lanStep.status === 'running')) {
            updateStep('lan_sync', 'done', '');
            lanSuccess = true;
          }
        } catch (e) {
          updateStep('lan_sync', 'error', 'Error: ' + String(e));
        }

        if (lanSuccess) {
          updateStep('relay_handshake', 'skipped', '局域网已连接，无需外网穿透');
          updateStep('relay_sync', 'skipped', '');
          pairingDone.value = true;
          pairingError.value = false;
          return;
        }
      }

      // Step 4: 局域网失败，尝试外网隧道握手 (Relay Handshake)
      if (window.appAPI.relayHandshake) {
        updateStep('relay_handshake', 'running', '');
        try {
          const timeoutPromise = new Promise((_, reject) => setTimeout(() => reject(new Error('Relay Timeout')), 15000));
          await Promise.race([
            window.appAPI.relayHandshake(payload.device_id, payload.public_key),
            timeoutPromise
          ]);
          updateStep('relay_handshake', 'done', '');
        } catch (e) {
          console.warn('Relay handshake failed', e);
          const errStr = String(e);
          if (!errStr.includes('ERR-PAIRING-03')) {
            updateStep('relay_handshake', 'error', 'Error: ' + errStr);
          } else {
            updateStep('relay_handshake', 'error', 'Error: PC 未响应握手');
          }
          pairingDone.value = true;
          pairingError.value = true;
          return; // If handshake fails, no point in syncing
        }
      }

      // Step 5: 外网隧道同步 (Relay Sync)
      if (window.appAPI.triggerMobileSync) {
        updateStep('relay_sync', 'running', '');
        try {
          const syncTimeout = new Promise((_, reject) => setTimeout(() => reject(new Error('Sync Timeout')), 45000));
          const relayPayload = { ...payload, skip_relay: false, local_ips: [] }; // Force Relay
          
          await Promise.race([
            window.appAPI.triggerMobileSync(relayPayload),
            syncTimeout
          ]);
          
          const relayStep = pairingSteps.value.find(s => s.id === 'relay_sync');
          if (relayStep && (relayStep.status === 'done' || relayStep.status === 'running')) {
            updateStep('relay_sync', 'done', '');
          }
          pairingDone.value = true;
          pairingError.value = false;
        } catch (e) {
          updateStep('relay_sync', 'error', 'Error: ' + String(e));
          pairingDone.value = true;
          pairingError.value = true;
        }
      }
