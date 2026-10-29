package org.bobagent.scanner

import android.app.Activity
import androidx.activity.result.ActivityResult
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import com.google.zxing.client.android.Intents
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions

/** Camera starts only after the explicit native scan command. ZXing is bundled;
 * no remote image, cloud decoder, saved photo, credential or network is involved.
 * Tauri's ActivityResult bridge owns callback registration across Activity state.
 */
@TauriPlugin
class PairingScannerPlugin(private val host: Activity) : Plugin(host) {
    private var active = false

    @Command
    fun scanPairingCode(invoke: Invoke) {
        if (active) {
            respond(invoke, "busy")
            return
        }
        active = true
        try {
            val promptId = host.resources.getIdentifier("bob_pairing_scan_prompt", "string", host.packageName)
            if (promptId == 0) throw IllegalStateException()
            val options = ScanOptions()
                .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                .setBeepEnabled(false)
                .setBarcodeImageEnabled(false)
                .addExtra(Intents.Scan.SHOW_MISSING_CAMERA_PERMISSION_DIALOG, false)
                .setTimeout(120000L)
                .setOrientationLocked(false)
                .setPrompt(host.getString(promptId))
            // CaptureActivity asks CAMERA permission on first explicit launch.
            // The intent targets our bundled decoder, not a third-party app.
            startActivityForResult(invoke, options.createScanIntent(host), "scanResult")
        } catch (_: Exception) {
            active = false
            respond(invoke, "unavailable")
        }
    }

    @ActivityCallback
    fun scanResult(invoke: Invoke, result: ActivityResult) {
        active = false
        try {
            if (result.data?.getBooleanExtra(Intents.Scan.MISSING_CAMERA_PERMISSION, false) == true) {
                respond(invoke, "permission_denied")
                return
            }
            val contents = ScanContract().parseResult(result.resultCode, result.data).contents
            if (contents == null) {
                respond(invoke, "cancelled")
            } else if (contents.isEmpty() || contents.toByteArray(Charsets.UTF_8).size > 8192 || contents.contains('\u0000')) {
                respond(invoke, "invalid")
            } else {
                respond(invoke, "ok", contents)
            }
        } catch (_: Exception) {
            respond(invoke, "invalid")
        }
    }

    private fun respond(invoke: Invoke, status: String, code: String? = null) {
        val response = JSObject()
        response.put("status", status)
        if (code != null) response.put("code", code)
        invoke.resolve(response)
    }
}
