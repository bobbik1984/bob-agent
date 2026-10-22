package org.bobbik.devicevault

import android.app.Activity
import android.app.KeyguardManager
import android.content.Context
import android.hardware.biometrics.BiometricManager.Authenticators
import android.hardware.biometrics.BiometricPrompt
import android.os.Build
import android.os.CancellationSignal
import android.os.Handler
import android.os.Looper
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.security.KeyStore
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

@InvokeArg
class UnlockArgs { var envelope: String? = null }

/** Auth-bound encryption, not a boolean biometric check followed by an unprotected read. */
@TauriPlugin
class DeviceVaultPlugin(private val activity: Activity) : Plugin(activity) {
    private val alias = "bob.remote-agent.v1"
    private var active: CancellationSignal? = null
    private var cancelPending: (() -> Unit)? = null
    private val handler = Handler(Looper.getMainLooper())

    @Suppress("DEPRECATION")
    override fun onDestroy() {
        cancelPending?.invoke()
        super.onDestroy()
    }

    @Command
    fun unlock(invoke: Invoke) {
        val args = invoke.parseArgs(UnlockArgs::class.java)
        activity.runOnUiThread {
            fun fail(code: String) {
                invoke.resolve(JSObject().put("key", "").put("envelope", "").put("error", code))
            }
            if (Build.VERSION.SDK_INT < 30) { fail("REMOTE_SYSTEM_UNSUPPORTED"); return@runOnUiThread }
            if (active != null) { fail("REMOTE_SYSTEM_AUTH_BUSY"); return@runOnUiThread }
            val keyguard = activity.getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager
            if (!keyguard.isDeviceSecure) { fail("REMOTE_SYSTEM_LOCK_REQUIRED"); return@runOnUiThread }
            try {
                val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
                val existing = args.envelope
                if (!store.containsAlias(alias)) {
                    // Never replace a lost/inactivated key when ciphertext already exists.
                    if (existing != null) { fail("REMOTE_SYSTEM_KEY_UNAVAILABLE"); return@runOnUiThread }
                    KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
                        init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                            .setKeySize(256).setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                            .setUserAuthenticationRequired(true)
                            .setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG or KeyProperties.AUTH_DEVICE_CREDENTIAL)
                            .build())
                        generateKey()
                    }
                }
                val key = store.getKey(alias, null) as SecretKey
                val cipher = Cipher.getInstance("AES/GCM/NoPadding")
                val wrapped = existing?.let { Base64.decode(it, Base64.NO_WRAP) }
                if (wrapped != null) {
                    require(wrapped.size == 60)
                    cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, wrapped.copyOfRange(0, 12)))
                } else { cipher.init(Cipher.ENCRYPT_MODE, key) }
                val signal = CancellationSignal()
                active = signal
                val timeout = Runnable {
                    if (active === signal) {
                        active = null
                        cancelPending = null
                        signal.cancel()
                        fail("REMOTE_SYSTEM_AUTH_CANCELLED")
                    }
                }
                cancelPending = { handler.removeCallbacks(timeout); timeout.run() }
                handler.postDelayed(timeout, 120_000)
                val prompt = BiometricPrompt.Builder(activity)
                    .setTitle(activity.getString(R.string.bob_vault_unlock))
                    .setSubtitle(activity.getString(R.string.bob_vault_subtitle))
                    .setAllowedAuthenticators(Authenticators.BIOMETRIC_STRONG or Authenticators.DEVICE_CREDENTIAL)
                    .build()
                prompt.authenticate(BiometricPrompt.CryptoObject(cipher), signal, activity.mainExecutor,
                    object : BiometricPrompt.AuthenticationCallback() {
                        override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                            if (active !== signal) return
                            active = null
                            cancelPending = null
                            handler.removeCallbacks(timeout)
                            fail(if (errorCode == BiometricPrompt.BIOMETRIC_ERROR_USER_CANCELED ||
                                errorCode == BiometricPrompt.BIOMETRIC_ERROR_CANCELED)
                                "REMOTE_SYSTEM_AUTH_CANCELLED" else "REMOTE_SYSTEM_AUTH_FAILED")
                        }
                        override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                            if (active !== signal) return
                            active = null
                            cancelPending = null
                            handler.removeCallbacks(timeout)
                            var plain: ByteArray? = null
                            try {
                                val authenticated = result.cryptoObject?.cipher ?: error("Missing crypto object")
                                val decrypted = if (wrapped != null) authenticated.doFinal(wrapped.copyOfRange(12, wrapped.size))
                                    else ByteArray(32).also { SecureRandom().nextBytes(it) }
                                plain = decrypted
                                require(decrypted.size == 32)
                                val envelope = existing ?: Base64.encodeToString(authenticated.iv + authenticated.doFinal(decrypted), Base64.NO_WRAP)
                                invoke.resolve(JSObject().put("key", Base64.encodeToString(decrypted, Base64.NO_WRAP))
                                    .put("envelope", envelope))
                            } catch (_: Exception) { fail("REMOTE_SYSTEM_KEY_UNAVAILABLE") }
                            finally { plain?.fill(0) }
                        }
                    })
            } catch (_: Exception) {
                if (cancelPending != null) cancelPending?.invoke()
                else { active = null; fail("REMOTE_SYSTEM_KEY_UNAVAILABLE") }
            }
        }
    }
}
