package org.bobagent.vault

import android.app.Activity
import android.app.KeyguardManager
import android.content.Context
import android.content.Intent
import android.hardware.biometrics.BiometricManager
import android.hardware.biometrics.BiometricPrompt
import android.os.Build
import android.os.CancellationSignal
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.system.Os
import android.system.OsConstants
import androidx.activity.result.ActivityResult
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import java.util.concurrent.atomic.AtomicBoolean
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

@InvokeArg
class VaultWriteArgs { var bytes: IntArray = intArrayOf() }

@InvokeArg
class SecretWriteArgs {
    var slot: String = ""
    var bytes: IntArray = intArrayOf()
}

@InvokeArg
class SecretSlotArgs {
    var slot: String = ""
}

@InvokeArg
class HttpsRequestArgs {
    var url: String = ""
    var method: String = "POST"
    var headers: Array<Array<String>> = emptyArray()
    var body: String? = null
    var timeout_ms: Int = 45000
}

@InvokeArg
class DeviceAuthArgs {
    var title: String = "Bob"
    var subtitle: String = ""
}

/** Rust-only native bridge; its Rust invoke handler rejects all WebView calls.
 * Plaintext never enters files, preferences or logs. Android backup excludes it.
 */
@TauriPlugin
class DeviceVaultPlugin(private val host: Activity) : Plugin(host) {
    companion object {
        private const val LIMIT = 65536
        private const val OVERHEAD = 4 + 12 + 16
        private val MAGIC = byteArrayOf(66, 79, 66, 1)
        private val LOCK = Any()
        private val SLOT_REGEX = Regex("^[A-Za-z0-9_-]{1,128}$")
    }
    private val alias get() = host.packageName + ".home-identity-v1"
    private val secretAlias get() = host.packageName + ".provider-secret-v1"
    private val aad get() = (host.packageName + ":bob-home-identity-v1").toByteArray(Charsets.UTF_8)
    private fun secretAad(slot: String) =
        (host.packageName + ":bob-provider-secret-v1:" + slot).toByteArray(Charsets.UTF_8)

    private fun keyForAlias(keyAlias: String, create: Boolean): SecretKey {
        check(Build.VERSION.SDK_INT >= 28)
        val keys = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        if (keys.containsAlias(keyAlias)) return keys.getKey(keyAlias, null) as SecretKey
        check(create)
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
            init(KeyGenParameterSpec.Builder(keyAlias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setKeySize(256)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setRandomizedEncryptionRequired(true)
                .setUnlockedDeviceRequired(true)
                .build())
        }.generateKey()
    }
    private fun key(create: Boolean): SecretKey = keyForAlias(alias, create)
    private fun slot(): File {
        // Android owns aliases such as /data/user/0; resolve the trusted OS
        // directory first rather than rejecting those system aliases.
        val root = host.noBackupFilesDir.canonicalFile
        check(OsConstants.S_ISDIR(Os.lstat(root.path).st_mode))
        return File(root, "home-identity.gcm")
    }
    private fun secretSlotFile(slotName: String): File {
        val clean = slotName.trim()
        check(SLOT_REGEX.matches(clean))
        val root = host.noBackupFilesDir.canonicalFile
        check(OsConstants.S_ISDIR(Os.lstat(root.path).st_mode))
        return File(root, "secret-$clean.gcm")
    }
    private fun regular(file: File) {
        check(OsConstants.S_ISREG(Os.lstat(file.path).st_mode))
        check(file.canonicalFile == file.absoluteFile)
    }
    private fun dispatch(invoke: Invoke, block: () -> JSObject) {
        Thread {
            try { invoke.resolve(synchronized(LOCK) { block() }) }
            catch (_: Exception) { invoke.reject("VAULT_UNAVAILABLE") }
        }.start()
    }
    @Command
    fun storeNew(invoke: Invoke) = dispatch(invoke) {
        val values = invoke.parseArgs(VaultWriteArgs::class.java).bytes
        check(values.isNotEmpty() && values.size <= LIMIT && values.all { it in 0..255 })
        val plain = ByteArray(values.size) { values[it].toByte() }
        values.fill(0)
        try {
            val target = slot()
            val pending = File(target.parentFile, target.name + ".pending")
            check(!target.exists() && !pending.exists())
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.ENCRYPT_MODE, key(true))
            cipher.updateAAD(aad)
            check(cipher.iv.size == 12)
            val encrypted = MAGIC + cipher.iv + cipher.doFinal(plain)
            check(pending.createNewFile())
            // Ambiguous writes retain pending and never overwrite an identity.
            FileOutputStream(pending).use { it.write(encrypted); it.fd.sync() }
            check(pending.renameTo(target))
            JSObject().apply { put("stored", true) }
        } finally { plain.fill(0) }
    }
    @Command
    fun readIdentity(invoke: Invoke) = dispatch(invoke) {
        val target = slot()
        val pending = File(target.parentFile, target.name + ".pending")
        check(!pending.exists())
        if (!target.exists()) JSObject().apply { put("bytes", JSONObject.NULL) }
        else {
            regular(target)
            check(target.length() in (OVERHEAD + 1).toLong()..(LIMIT + OVERHEAD).toLong())
            val sealed = FileInputStream(target).use { input ->
                val buffer = ByteArray(LIMIT + OVERHEAD + 1)
                var count = 0
                while (count < buffer.size) {
                    val size = input.read(buffer, count, buffer.size - count)
                    if (size < 0) break
                    count += size
                }
                buffer.copyOf(count)
            }
            check(sealed.size in (OVERHEAD + 1)..(LIMIT + OVERHEAD))
            check(sealed.copyOfRange(0, 4).contentEquals(MAGIC))
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, key(false), GCMParameterSpec(128, sealed.copyOfRange(4, 16)))
            cipher.updateAAD(aad)
            val plain = cipher.doFinal(sealed, 16, sealed.size - 16)
            try {
                check(plain.isNotEmpty() && plain.size <= LIMIT)
                JSObject().apply { put("bytes", JSONArray(plain.map { it.toInt() and 255 })) }
            } finally { plain.fill(0) }
        }
    }
    @Command
    fun storeSecret(invoke: Invoke) = dispatch(invoke) {
        val args = invoke.parseArgs(SecretWriteArgs::class.java)
        val cleanSlot = args.slot.trim()
        val values = args.bytes
        check(values.isNotEmpty() && values.size <= LIMIT && values.all { it in 0..255 })
        val plain = ByteArray(values.size) { values[it].toByte() }
        values.fill(0)
        try {
            val target = secretSlotFile(cleanSlot)
            val pending = File(target.parentFile, target.name + ".pending")
            if (pending.exists()) pending.delete()
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.ENCRYPT_MODE, keyForAlias(secretAlias, true))
            cipher.updateAAD(secretAad(cleanSlot))
            check(cipher.iv.size == 12)
            val encrypted = MAGIC + cipher.iv + cipher.doFinal(plain)
            check(pending.createNewFile())
            FileOutputStream(pending).use { it.write(encrypted); it.fd.sync() }
            if (target.exists()) target.delete()
            check(pending.renameTo(target))
            JSObject().apply { put("stored", true) }
        } finally { plain.fill(0) }
    }
    @Command
    fun readSecret(invoke: Invoke) = dispatch(invoke) {
        val cleanSlot = invoke.parseArgs(SecretSlotArgs::class.java).slot.trim()
        val target = secretSlotFile(cleanSlot)
        if (!target.exists()) JSObject().apply { put("bytes", JSONObject.NULL) }
        else {
            regular(target)
            check(target.length() in (OVERHEAD + 1).toLong()..(LIMIT + OVERHEAD).toLong())
            val sealed = FileInputStream(target).use { input ->
                val buffer = ByteArray(LIMIT + OVERHEAD + 1)
                var count = 0
                while (count < buffer.size) {
                    val size = input.read(buffer, count, buffer.size - count)
                    if (size < 0) break
                    count += size
                }
                buffer.copyOf(count)
            }
            check(sealed.size in (OVERHEAD + 1)..(LIMIT + OVERHEAD))
            check(sealed.copyOfRange(0, 4).contentEquals(MAGIC))
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(
                Cipher.DECRYPT_MODE,
                keyForAlias(secretAlias, false),
                GCMParameterSpec(128, sealed.copyOfRange(4, 16))
            )
            cipher.updateAAD(secretAad(cleanSlot))
            val plain = cipher.doFinal(sealed, 16, sealed.size - 16)
            try {
                check(plain.isNotEmpty() && plain.size <= LIMIT)
                JSObject().apply { put("bytes", JSONArray(plain.map { it.toInt() and 255 })) }
            } finally { plain.fill(0) }
        }
    }
    @Command
    fun clearSecret(invoke: Invoke) = dispatch(invoke) {
        val cleanSlot = invoke.parseArgs(SecretSlotArgs::class.java).slot.trim()
        val target = secretSlotFile(cleanSlot)
        val pending = File(target.parentFile, target.name + ".pending")
        if (pending.exists()) pending.delete()
        if (target.exists()) target.delete()
        JSObject().apply { put("cleared", true) }
    }
    @Command
    fun httpsRequest(invoke: Invoke) {
        Thread {
            try {
                val args = invoke.parseArgs(HttpsRequestArgs::class.java)
                val conn = (URL(args.url).openConnection() as HttpURLConnection).apply {
                    requestMethod = args.method
                    connectTimeout = args.timeout_ms.coerceIn(1000, 120000)
                    readTimeout = args.timeout_ms.coerceIn(1000, 120000)
                    useCaches = false
                    instanceFollowRedirects = false
                }
                for (pair in args.headers) {
                    if (pair.size >= 2) {
                        conn.setRequestProperty(pair[0], pair[1])
                    }
                }
                val payload = args.body
                if (payload != null) {
                    conn.doOutput = true
                    val bytes = payload.toByteArray(Charsets.UTF_8)
                    conn.outputStream.use { out ->
                        out.write(bytes)
                        out.flush()
                    }
                }
                val code = conn.responseCode
                val stream = if (code in 200..299) conn.inputStream else (conn.errorStream ?: conn.inputStream)
                val text = stream?.bufferedReader(Charsets.UTF_8)?.use { it.readText() } ?: ""
                conn.disconnect()
                invoke.resolve(JSObject().apply {
                    put("status", code)
                    put("body", text)
                })
            } catch (_: Exception) {
                invoke.reject("PROVIDER_HTTP_FAILED")
            }
        }.start()
    }

    private fun isDeviceSecureAndEnrolled(): Boolean {
        val km = host.getSystemService(Context.KEYGUARD_SERVICE) as? KeyguardManager
        val deviceSecure = km?.isDeviceSecure == true
        val bioOk = if (Build.VERSION.SDK_INT >= 30) {
            val bm = host.getSystemService(BiometricManager::class.java)
            bm?.canAuthenticate(
                BiometricManager.Authenticators.BIOMETRIC_STRONG or
                    BiometricManager.Authenticators.BIOMETRIC_WEAK or
                    BiometricManager.Authenticators.DEVICE_CREDENTIAL
            ) == BiometricManager.BIOMETRIC_SUCCESS
        } else {
            false
        }
        return deviceSecure || bioOk
    }

    @Command
    fun checkDeviceAuth(invoke: Invoke) {
        try {
            val enrolled = isDeviceSecureAndEnrolled()
            invoke.resolve(JSObject().apply {
                put("supported", true)
                put("enrolled", enrolled)
            })
        } catch (_: Exception) {
            invoke.resolve(JSObject().apply {
                put("supported", true)
                put("enrolled", false)
            })
        }
    }

    @Command
    fun authenticateDeviceOwner(invoke: Invoke) {
        val args = try {
            invoke.parseArgs(DeviceAuthArgs::class.java)
        } catch (_: Exception) {
            DeviceAuthArgs()
        }
        val cleanTitle = args.title.ifBlank { "Bob" }
        val cleanSubtitle = args.subtitle.trim()

        if (!isDeviceSecureAndEnrolled()) {
            invoke.resolve(JSObject().apply {
                put("authenticated", false)
                put("status", "not_enrolled")
            })
            return
        }

        if (Build.VERSION.SDK_INT >= 29) {
            host.runOnUiThread {
                val settled = AtomicBoolean(false)
                try {
                    val builder = BiometricPrompt.Builder(host)
                        .setTitle(cleanTitle)
                    if (cleanSubtitle.isNotEmpty()) {
                        builder.setSubtitle(cleanSubtitle)
                    }
                    if (Build.VERSION.SDK_INT >= 30) {
                        builder.setAllowedAuthenticators(
                            BiometricManager.Authenticators.BIOMETRIC_STRONG or
                                BiometricManager.Authenticators.BIOMETRIC_WEAK or
                                BiometricManager.Authenticators.DEVICE_CREDENTIAL
                        )
                    } else {
                        @Suppress("DEPRECATION")
                        builder.setDeviceCredentialAllowed(true)
                    }
                    val callback = object : BiometricPrompt.AuthenticationCallback() {
                        override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                            if (settled.compareAndSet(false, true)) {
                                invoke.resolve(JSObject().apply {
                                    put("authenticated", true)
                                    put("status", "ok")
                                })
                            }
                        }

                        override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                            if (settled.compareAndSet(false, true)) {
                                val mapped = when (errorCode) {
                                    BiometricPrompt.BIOMETRIC_ERROR_CANCELED,
                                    BiometricPrompt.BIOMETRIC_ERROR_USER_CANCELED -> "cancelled"
                                    BiometricPrompt.BIOMETRIC_ERROR_NO_BIOMETRICS,
                                    14 -> "not_enrolled"
                                    else -> "failed"
                                }
                                invoke.resolve(JSObject().apply {
                                    put("authenticated", false)
                                    put("status", mapped)
                                })
                            }
                        }
                    }
                    builder.build().authenticate(
                        CancellationSignal(),
                        host.mainExecutor,
                        callback
                    )
                } catch (_: Exception) {
                    if (settled.compareAndSet(false, true)) {
                        launchConfirmCredentialIntent(invoke, cleanTitle, cleanSubtitle)
                    }
                }
            }
            return
        }

        launchConfirmCredentialIntent(invoke, cleanTitle, cleanSubtitle)
    }

    private fun launchConfirmCredentialIntent(invoke: Invoke, title: String, subtitle: String) {
        try {
            val km = host.getSystemService(Context.KEYGUARD_SERVICE) as? KeyguardManager
            @Suppress("DEPRECATION")
            val intent: Intent? = km?.createConfirmDeviceCredentialIntent(title, subtitle.ifEmpty { null })
            if (intent == null) {
                invoke.resolve(JSObject().apply {
                    put("authenticated", false)
                    put("status", "not_enrolled")
                })
                return
            }
            startActivityForResult(invoke, intent, "deviceCredentialResult")
        } catch (_: Exception) {
            invoke.resolve(JSObject().apply {
                put("authenticated", false)
                put("status", "failed")
            })
        }
    }

    @ActivityCallback
    fun deviceCredentialResult(invoke: Invoke, result: ActivityResult) {
        val ok = result.resultCode == Activity.RESULT_OK
        invoke.resolve(JSObject().apply {
            put("authenticated", ok)
            put("status", if (ok) "ok" else "cancelled")
        })
    }
}
