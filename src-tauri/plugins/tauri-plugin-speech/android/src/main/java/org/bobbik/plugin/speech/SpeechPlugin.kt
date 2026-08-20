package org.bobbik.plugin.speech

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import android.Manifest
import android.content.pm.PackageManager
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Plugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject

@TauriPlugin
class SpeechPlugin(private val activity: Activity): Plugin(activity) {

    private var speechRecognizer: SpeechRecognizer? = null
    
    override fun load(webView: android.webkit.WebView) {
        super.load(webView)
        activity.runOnUiThread {
            speechRecognizer = SpeechRecognizer.createSpeechRecognizer(activity)
            speechRecognizer?.setRecognitionListener(object : RecognitionListener {
                override fun onReadyForSpeech(params: Bundle?) {}
                override fun onBeginningOfSpeech() {}
                override fun onRmsChanged(rmsdB: Float) {}
                override fun onBufferReceived(buffer: ByteArray?) {}
                override fun onEndOfSpeech() {}
                override fun onError(error: Int) {
                    val ret = JSObject()
                    ret.put("error", "Error code: $error")
                    trigger("speech_error", ret)
                }
                
                override fun onResults(results: Bundle?) {
                    val matches = results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)
                    if (!matches.isNullOrEmpty()) {
                        val ret = JSObject()
                        ret.put("text", matches[0])
                        trigger("speech_final", ret)
                    }
                }
                
                override fun onPartialResults(partialResults: Bundle?) {
                    val matches = partialResults?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)
                    if (!matches.isNullOrEmpty()) {
                        val ret = JSObject()
                        ret.put("text", matches[0])
                        trigger("speech_partial", ret)
                    }
                }
                
                override fun onEvent(eventType: Int, params: Bundle?) {}
            })
        }
    }

    @Command
    fun start_listening(invoke: Invoke) {
        if (ContextCompat.checkSelfPermission(activity, Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            ActivityCompat.requestPermissions(activity, arrayOf(Manifest.permission.RECORD_AUDIO), 1)
            invoke.reject("Permission denied. Requested permission, please retry.")
            return
        }

        val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
            putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
            putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
            putExtra(RecognizerIntent.EXTRA_MAX_RESULTS, 1)
        }
        
        activity.runOnUiThread {
            speechRecognizer?.startListening(intent)
            invoke.resolve()
        }
    }

    @Command
    fun stop_listening(invoke: Invoke) {
        activity.runOnUiThread {
            speechRecognizer?.stopListening()
            invoke.resolve()
        }
    }
}
