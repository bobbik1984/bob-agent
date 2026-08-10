package bob.agent

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import java.io.File
import java.io.FileOutputStream
import java.util.UUID

class ShareActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        
        handleIntent(intent)
        
        // Launch main activity
        val mainIntent = Intent(this, MainActivity::class.java)
        mainIntent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        startActivity(mainIntent)
        finish()
    }

    private fun handleIntent(intent: Intent) {
        val action = intent.action
        val type = intent.type

        if (Intent.ACTION_SEND == action && type != null) {
            if ("text/plain" == type) {
                handleSendText(intent)
            } else if (type.startsWith("image/")) {
                handleSendImage(intent)
            }
        }
    }

    private fun handleSendText(intent: Intent) {
        intent.getStringExtra(Intent.EXTRA_TEXT)?.let { sharedText ->
            saveToCache("shared_text_${UUID.randomUUID()}.txt", sharedText.toByteArray())
        }
    }

    private fun handleSendImage(intent: Intent) {
        // Suppress deprecation for getParcelableExtra on older APIs
        @Suppress("DEPRECATION")
        val imageUri = intent.getParcelableExtra<Uri>(Intent.EXTRA_STREAM)
        imageUri?.let { uri ->
            saveUriToCache(uri)
        }
    }

    private fun saveUriToCache(uri: Uri) {
        try {
            val inputStream = contentResolver.openInputStream(uri)
            val originalName = getFileName(uri) ?: "shared_image.png"
            // A UUID prefix prevents two shares with the same display name from
            // overwriting each other. Rust strips the prefix when archiving.
            val fileName = "${UUID.randomUUID()}__${sanitizeFileName(originalName)}"
            val targetDir = File(cacheDir, "shared_incoming")
            if (!targetDir.exists()) targetDir.mkdirs()
            
            val outFile = File(targetDir, fileName)
            val outputStream = FileOutputStream(outFile)
            
            inputStream?.copyTo(outputStream)
            inputStream?.close()
            outputStream.close()
        } catch (e: Exception) {
            e.printStackTrace()
        }
    }

    private fun saveToCache(fileName: String, data: ByteArray) {
        try {
            val targetDir = File(cacheDir, "shared_incoming")
            if (!targetDir.exists()) targetDir.mkdirs()
            
            val outFile = File(targetDir, fileName)
            outFile.writeBytes(data)
        } catch (e: Exception) {
            e.printStackTrace()
        }
    }

    private fun sanitizeFileName(value: String): String {
        val cleaned = value.replace(Regex("[^\\p{L}\\p{N}._-]"), "_")
            .trim('.', '_')
            .take(120)
        return if (cleaned.isBlank()) "shared_image.png" else cleaned
    }

    private fun getFileName(uri: Uri): String? {
        var result: String? = null
        if (uri.scheme == "content") {
            val cursor = contentResolver.query(uri, null, null, null, null)
            try {
                if (cursor != null && cursor.moveToFirst()) {
                    val index = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    if (index >= 0) {
                        result = cursor.getString(index)
                    }
                }
            } catch (e: Exception) {
                // ignore
            } finally {
                cursor?.close()
            }
        }
        if (result == null) {
            result = uri.path
            val cut = result?.lastIndexOf('/')
            if (cut != null && cut != -1) {
                result = result?.substring(cut + 1)
            }
        }
        return result
    }
}
