package org.bobbik.bobagent

import android.content.Intent
import android.os.Bundle
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import android.util.Log
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import app.tauri.plugin.Invoke

@InvokeArg
class SpeechArgs {
    var language: String = "zh-CN"
}

@TauriPlugin
class SpeechRecognizerPlugin : Plugin() {
    private var speechRecognizer: SpeechRecognizer? = null
    private var isListening = false

    override fun load() {
        super.load()
        Log.d("BobSpeech", "SpeechRecognizerPlugin loaded")
    }

    @Command
    fun startListening(invoke: Invoke) {
        if (isListening) {
            invoke.resolve(JSObject().apply { put("status", "already_listening") })
            return
        }

        // 动态权限检查 (Runtime Permissions)
        val audioPermission = android.Manifest.permission.RECORD_AUDIO
        if (androidx.core.content.ContextCompat.checkSelfPermission(activity, audioPermission) != android.content.pm.PackageManager.PERMISSION_GRANTED) {
            androidx.core.app.ActivityCompat.requestPermissions(activity, arrayOf(audioPermission), 1)
            invoke.reject("require_permission")
            return
        }

        activity.runOnUiThread {
            try {
                if (speechRecognizer == null) {
                    speechRecognizer = SpeechRecognizer.createSpeechRecognizer(activity)
                }

                val args = invoke.parseArgs(SpeechArgs::class.java)
                val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
                    putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
                    putExtra(RecognizerIntent.EXTRA_LANGUAGE, args.language)
                    putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
                }

                speechRecognizer?.setRecognitionListener(object : RecognitionListener {
                    override fun onReadyForSpeech(params: Bundle?) {
                        Log.d("BobSpeech", "Ready for speech")
                        trigger("speech:ready", JSObject())
                    }

                    override fun onBeginningOfSpeech() {
                        isListening = true
                        trigger("speech:begin", JSObject())
                    }

                    override fun onRmsChanged(rmsdB: Float) {
                        val event = JSObject().apply { put("rms", rmsdB) }
                        trigger("speech:rms", event)
                    }

                    override fun onBufferReceived(buffer: ByteArray?) {}

                    override fun onEndOfSpeech() {
                        isListening = false
                        trigger("speech:end", JSObject())
                    }

                    override fun onError(error: Int) {
                        isListening = false
                        val errorMsg = when (error) {
                            SpeechRecognizer.ERROR_AUDIO -> "Audio recording error"
                            SpeechRecognizer.ERROR_CLIENT -> "Client side error"
                            SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> "Insufficient permissions"
                            SpeechRecognizer.ERROR_NETWORK -> "Network error"
                            SpeechRecognizer.ERROR_NETWORK_TIMEOUT -> "Network timeout"
                            SpeechRecognizer.ERROR_NO_MATCH -> "No match"
                            SpeechRecognizer.ERROR_RECOGNIZER_BUSY -> "RecognitionService busy"
                            SpeechRecognizer.ERROR_SERVER -> "Error from server"
                            SpeechRecognizer.ERROR_SPEECH_TIMEOUT -> "No speech input"
                            else -> "Unknown error"
                        }
                        Log.e("BobSpeech", "Error: $errorMsg")
                        val event = JSObject().apply { put("error", errorMsg); put("code", error) }
                        trigger("speech:error", event)
                    }

                    override fun onResults(results: Bundle?) {
                        isListening = false
                        val matches = results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)
                        if (!matches.isNullOrEmpty()) {
                            val event = JSObject().apply { put("text", matches[0]) }
                            trigger("speech:result", event)
                        }
                    }

                    override fun onPartialResults(partialResults: Bundle?) {
                        val matches = partialResults?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)
                        if (!matches.isNullOrEmpty()) {
                            val event = JSObject().apply { put("text", matches[0]) }
                            trigger("speech:partial", event)
                        }
                    }

                    override fun onEvent(eventType: Int, params: Bundle?) {}
                })

                speechRecognizer?.startListening(intent)
                invoke.resolve(JSObject().apply { put("status", "started") })
            } catch (e: Exception) {
                Log.e("BobSpeech", "Failed to start listening", e)
                invoke.reject("Failed to start listening: ${e.message}")
            }
        }
    }

    @Command
    fun stopListening(invoke: Invoke) {
        activity.runOnUiThread {
            try {
                speechRecognizer?.stopListening()
                isListening = false
                invoke.resolve(JSObject().apply { put("status", "stopped") })
            } catch (e: Exception) {
                invoke.reject("Failed to stop listening: ${e.message}")
            }
        }
    }

    @Command
    fun cancelListening(invoke: Invoke) {
        activity.runOnUiThread {
            try {
                speechRecognizer?.cancel()
                isListening = false
                invoke.resolve(JSObject().apply { put("status", "cancelled") })
            } catch (e: Exception) {
                invoke.reject("Failed to cancel listening: ${e.message}")
            }
        }
    }
}
